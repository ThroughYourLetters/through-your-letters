import { randomUUID } from "node:crypto";

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export async function checkApiAvailability({
  baseUrl = "https://through-your-letters-api.onrender.com",
  readinessTimeoutMs = 30000,
  protectedTimeoutMs = 10000,
  retryDelayMs = 2000,
} = {}) {
  let ready = false;
  for (let attempt = 0; attempt < 2; attempt++) {
    try {
      const response = await fetch(new URL("/health", baseUrl), {
        redirect: "error",
        signal: AbortSignal.timeout(readinessTimeoutMs),
      });
      const body = response.ok ? await response.json() : null;
      ready =
        body?.status === "healthy" &&
        body?.database === "up" &&
        body?.cache_consistency === "up";
    } catch {
      ready = false;
    }
    if (ready) break;
    if (attempt === 0) await delay(retryDelayMs);
  }
  if (!ready) throw new Error("Public API readiness failed after two attempts");

  let response;
  let body;
  try {
    // An impossible account exercises the limiter before credential validation.
    response = await fetch(new URL("/api/v1/admin/login", baseUrl), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        email: `ttl-health-${randomUUID()}@example.invalid`,
        password: "not-a-real-account-credential",
      }),
      redirect: "error",
      signal: AbortSignal.timeout(protectedTimeoutMs),
    });
    body = await response.json();
  } catch {
    throw new Error("Protected API readiness request failed or timed out");
  }
  const admitted = response.status === 403 && body?.error === "Access denied";
  const retryAfter = response.headers.get("retry-after");
  const throttled =
    response.status === 429 &&
    body?.error === "Too many requests, please try again later" &&
    /^\d+$/.test(retryAfter ?? "") &&
    Number.isSafeInteger(Number(retryAfter)) &&
    Number(retryAfter) > 0;
  if (!admitted && !throttled) {
    throw new Error(`Protected API readiness failed (HTTP ${response.status})`);
  }
  return {
    checked_at: new Date().toISOString(),
    public_status: 200,
    protected_status: response.status,
  };
}

if (import.meta.main) {
  try {
    console.log(JSON.stringify(await checkApiAvailability()));
  } catch (error) {
    console.error(`::error::${error.message}`);
    process.exitCode = 1;
  }
}
