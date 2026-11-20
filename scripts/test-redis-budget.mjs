import assert from "node:assert/strict";
import { once } from "node:events";
import { createServer } from "node:http";
import { test, after } from "node:test";
import { checkRedisBudget } from "./check-redis-budget.mjs";

const databaseId = "11111111-1111-1111-1111-111111111111";
const email = "monitor@example.com";
const apiKey = "test-only-monitor-key";
let reply;
let observed;
const server = createServer((request, response) => {
  observed = {
    url: request.url,
    method: request.method,
    auth: request.headers.authorization,
  };
  response.writeHead(reply.status ?? 200, {
    "Content-Type": "application/json",
  });
  response.end(
    typeof reply.body === "string" ? reply.body : JSON.stringify(reply.body),
  );
});
server.listen(0, "127.0.0.1");
await once(server, "listening");
after(() => server.close());
const request = (url, options) => {
  assert.equal(new URL(url).origin, "https://api.upstash.com");
  assert.equal(options.redirect, "error");
  return fetch(
    `http://127.0.0.1:${server.address().port}${new URL(url).pathname}${new URL(url).search}`,
    options,
  );
};
const check = (now = new Date("2026-11-20T12:00:00Z")) =>
  checkRedisBudget({ email, apiKey, databaseId, request, now });
const usage = (monthly, today = 0) => {
  reply = {
    body: { total_monthly_requests: monthly, daily_net_commands: today },
  };
};

test("reads the provider's monthly total with authenticated GET", async () => {
  usage(0);
  const result = await check();
  assert.deepEqual(result.alerts, []);
  assert.equal(result.monthly, 0);
  assert.deepEqual(observed, {
    url: `/v2/redis/stats/${databaseId}?period=1d`,
    method: "GET",
    auth: `Basic ${Buffer.from(`${email}:${apiKey}`).toString("base64")}`,
  });
});

test("warns at the reserve and critical thresholds", async () => {
  for (const [monthly, alert] of [
    [34999, undefined],
    [35000, "Warning"],
    [44999, "Warning"],
    [45000, "Critical"],
    [50000, "Critical"],
  ]) {
    usage(monthly);
    const result = await check();
    assert.equal(result.alerts[0]?.split(":")[0], alert);
  }
});

test("alerts when today's usage would exhaust the remaining monthly budget", async () => {
  usage(30000, 3000);
  const result = await check();
  assert.equal(result.days_left, 10);
  assert.equal(result.projected, 60000);
  assert.match(result.alerts[0], /exceed 50000/);
  assert.deepEqual((await check(new Date("2026-11-30T23:00:00Z"))).alerts, []);
  assert.equal((await check(new Date("2028-02-28T23:00:00Z"))).days_left, 1);
});

test("fails closed on provider failures without exposing response bodies", async () => {
  for (const status of [401, 429, 503]) {
    reply = { status, body: `must-not-log-${apiKey}` };
    await assert.rejects(check(), {
      message: `Upstash usage request returned HTTP ${status}`,
    });
  }
  reply = { body: "not JSON" };
  await assert.rejects(check(), /not JSON/);
  for (const body of [
    {},
    null,
    { total_monthly_requests: "0", daily_net_commands: 0 },
    { total_monthly_requests: -1, daily_net_commands: 0 },
    { total_monthly_requests: 1, daily_net_commands: 2 },
  ]) {
    reply = { body };
    await assert.rejects(check(), /missing or inconsistent/);
  }
  await assert.rejects(
    checkRedisBudget({
      email,
      apiKey,
      databaseId,
      request: async () => {
        throw new Error(apiKey);
      },
    }),
    { message: "Upstash usage request failed or exceeded 15 seconds" },
  );
});

test("rejects incomplete credentials before making a request", async () => {
  await assert.rejects(
    checkRedisBudget({
      databaseId,
      request: () => assert.fail("must not request"),
    }),
    /Set UPSTASH_EMAIL/,
  );
});
