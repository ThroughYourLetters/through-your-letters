import { appendFile } from "node:fs/promises";
import policy from "../apps/api/config/runtime.json" with { type: "json" };

export async function checkRedisBudget({
  email,
  apiKey,
  databaseId,
  now = new Date(),
  request = fetch,
}) {
  if (!email || !apiKey || !/^[a-f0-9-]{36}$/.test(databaseId ?? "")) {
    throw new Error(
      "Set UPSTASH_EMAIL, UPSTASH_API_KEY, and UPSTASH_DATABASE_ID",
    );
  }
  let response;
  try {
    response = await request(
      `https://api.upstash.com/v2/redis/stats/${databaseId}?period=1d`,
      {
        headers: {
          Authorization: `Basic ${Buffer.from(`${email}:${apiKey}`).toString("base64")}`,
        },
        redirect: "error",
        signal: AbortSignal.timeout(15000),
      },
    );
  } catch {
    throw new Error("Upstash usage request failed or exceeded 15 seconds");
  }
  if (!response.ok) {
    throw new Error(`Upstash usage request returned HTTP ${response.status}`);
  }
  let stats;
  try {
    stats = await response.json();
  } catch {
    throw new Error("Upstash usage response is not JSON");
  }
  const monthly = stats?.total_monthly_requests;
  const today = stats?.daily_net_commands;
  if (
    !Number.isSafeInteger(monthly) ||
    monthly < 0 ||
    !Number.isSafeInteger(today) ||
    today < 0 ||
    today > monthly
  ) {
    throw new Error(
      "Upstash usage response has missing or inconsistent command totals",
    );
  }
  const budget = policy.redis_budget.monthly_commands;
  const warning = budget - policy.redis_budget.reserved_commands;
  const critical = Math.floor(budget * 0.9);
  const daysInMonth = new Date(
    Date.UTC(now.getUTCFullYear(), now.getUTCMonth() + 1, 0),
  ).getUTCDate();
  const daysLeft = daysInMonth - now.getUTCDate();
  // Today's partial count is a lower bound for a full day's usage.
  const projected = monthly + today * daysLeft;
  const alerts = [];
  if (monthly >= critical)
    alerts.push(`Critical: monthly commands reached ${critical}`);
  else if (monthly >= warning)
    alerts.push(`Warning: monthly commands reached ${warning}`);
  if (projected > budget)
    alerts.push(
      `Usage would exceed ${budget} if today's count repeats on each remaining day`,
    );
  return {
    checked_at: now.toISOString(),
    monthly,
    today,
    budget,
    warning,
    critical,
    days_left: daysLeft,
    projected,
    alerts,
  };
}

if (import.meta.main) {
  try {
    const result = await checkRedisBudget({
      email: process.env.UPSTASH_EMAIL,
      apiKey: process.env.UPSTASH_API_KEY,
      databaseId: process.env.UPSTASH_DATABASE_ID,
    });
    const summary = `Redis commands: ${result.monthly} / ${result.budget} this UTC month; ${result.today} today. If today's count repeats, month-end usage reaches ${result.projected}.`;
    console.log(JSON.stringify(result));
    if (process.env.GITHUB_STEP_SUMMARY) {
      await appendFile(
        process.env.GITHUB_STEP_SUMMARY,
        `${summary}\n\n${result.alerts.join("\n\n")}\n`,
      );
    }
    for (const alert of result.alerts) console.error(`::error::${alert}`);
    if (result.alerts.length) process.exitCode = 1;
  } catch (error) {
    console.error(`::error::${error.message}`);
    process.exitCode = 1;
  }
}
