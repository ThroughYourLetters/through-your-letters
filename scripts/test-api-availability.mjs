import assert from "node:assert/strict";
import { once } from "node:events";
import { createServer } from "node:http";
import { test } from "node:test";
import { checkApiAvailability } from "./check-api-availability.mjs";

async function fixture(
  t,
  {
    health = 200,
    protectedStatus = 403,
    protectedBody = { error: "Access denied" },
    retryAfter,
    protectedDelay = 0,
  } = {},
) {
  const requests = [];
  const server = createServer(async (request, response) => {
    let body = "";
    for await (const chunk of request) body += chunk;
    requests.push({ method: request.method, url: request.url, body });
    if (request.url === "/health") {
      response.writeHead(health, { "Content-Type": "application/json" });
      response.end(
        JSON.stringify({
          status: "healthy",
          database: "up",
          cache_consistency: "up",
        }),
      );
    } else {
      const send = () => {
        response.writeHead(protectedStatus, {
          "Content-Type": "application/json",
          ...(retryAfter ? { "Retry-After": retryAfter } : {}),
        });
        response.end(JSON.stringify(protectedBody));
      };
      if (protectedDelay) setTimeout(send, protectedDelay);
      else send();
    }
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  t.after(() => {
    server.closeAllConnections();
    server.close();
  });
  return {
    requests,
    run: () =>
      checkApiAvailability({
        baseUrl: `http://127.0.0.1:${server.address().port}`,
        readinessTimeoutMs: 100,
        protectedTimeoutMs: 40,
        retryDelayMs: 0,
      }),
  };
}

test("checks public readiness then one rejected diagnostic login", async (t) => {
  const f = await fixture(t);
  assert.equal((await f.run()).protected_status, 403);
  assert.deepEqual(
    f.requests.map(({ method, url }) => [method, url]),
    [
      ["GET", "/health"],
      ["POST", "/api/v1/admin/login"],
    ],
  );
  assert.match(
    JSON.parse(f.requests[1].body).email,
    /^ttl-health-.*@example\.invalid$/,
  );
});

test("an exhausted caller quota still proves the limiter answered", async (t) => {
  const f = await fixture(t, {
    protectedStatus: 429,
    protectedBody: { error: "Too many requests, please try again later" },
    retryAfter: "17",
  });
  assert.equal((await f.run()).protected_status, 429);
});

test("retries public readiness once and sends no login while it is down", async (t) => {
  const f = await fixture(t, { health: 503 });
  await assert.rejects(f.run(), /Public API readiness failed/);
  assert.deepEqual(
    f.requests.map((r) => r.method),
    ["GET", "GET"],
  );
});

test("a protected outage fails and the POST is never retried", async (t) => {
  const f = await fixture(t, {
    protectedStatus: 503,
    protectedBody: { error: "External service unavailable" },
  });
  await assert.rejects(f.run(), /HTTP 503/);
  assert.equal(f.requests.filter((r) => r.method === "POST").length, 1);
});

test("unexpected denial bodies and missing Retry-After do not pass", async (t) => {
  const denied = await fixture(t, {
    protectedBody: { error: "blocked at the edge" },
  });
  await assert.rejects(denied.run(), /HTTP 403/);
  const limited = await fixture(t, {
    protectedStatus: 429,
    protectedBody: { error: "Too many requests, please try again later" },
  });
  await assert.rejects(limited.run(), /HTTP 429/);
});

test("slow protected responses fail under the request deadline", async (t) => {
  const f = await fixture(t, { protectedDelay: 120 });
  await assert.rejects(f.run(), /failed or timed out/);
  assert.equal(f.requests.filter((r) => r.method === "POST").length, 1);
});
