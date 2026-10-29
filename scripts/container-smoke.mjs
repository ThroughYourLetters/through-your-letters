import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import { setTimeout as delay } from "node:timers/promises";

// Every service and record belongs to this disposable test network.
const prefix = `ttl-smoke-${process.pid}-${Date.now()}`;
const names = Object.fromEntries(
  ["db", "redis", "api", "web"].map((key) => [key, `${prefix}-${key}`]),
);
const apiImage = process.env.TTL_API_IMAGE || "ttl-api:test";
const webImage = process.env.TTL_WEB_IMAGE || "ttl-web:test";
const docker = (...args) => {
  const result = spawnSync("docker", args, {
    encoding: "utf8",
    maxBuffer: 4 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `docker ${args[0]} failed: ${result.stderr}`);
  return result.stdout.trim();
};
const origin = (name, port) => `http://${docker("port", name, `${port}/tcp`)}`;
async function waitForHttp(url, accepts) {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(2000) });
      if (await accepts(response)) return;
    } catch {
      /* Startup can precede the listener and migrations. */
    }
    await delay(500);
  }
  throw new Error(`Service did not become ready: ${url}`);
}
const apiHealthy = async (response) =>
  response.ok && (await response.json()).status === "healthy";
async function websocket(url) {
  await new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    const timer = setTimeout(() => {
      socket.close();
      reject(new Error("WebSocket upgrade timed out"));
    }, 10_000);
    socket.addEventListener(
      "open",
      () => {
        clearTimeout(timer);
        socket.close();
        resolve();
      },
      { once: true },
    );
    socket.addEventListener(
      "error",
      () => {
        clearTimeout(timer);
        reject(new Error("WebSocket upgrade failed"));
      },
      { once: true },
    );
  });
}

try {
  docker("network", "create", prefix);
  docker(
    "run",
    "--detach",
    "--name",
    names.db,
    "--network",
    prefix,
    "-e",
    "POSTGRES_USER=test",
    "-e",
    "POSTGRES_PASSWORD=test",
    "-e",
    "POSTGRES_DB=container_smoke",
    "postgis/postgis:17-3.5",
  );
  docker(
    "run",
    "--detach",
    "--name",
    names.redis,
    "--network",
    prefix,
    "redis:8.4.0-alpine",
  );
  const dbDeadline = Date.now() + 60_000;
  while (
    spawnSync("docker", [
      "exec",
      names.db,
      "pg_isready",
      "-h",
      "127.0.0.1",
      "-U",
      "test",
      "-d",
      "container_smoke",
    ]).status !== 0
  ) {
    assert.ok(Date.now() < dbDeadline, "PostgreSQL startup timed out");
    await delay(500);
  }
  const config = {
    DATABASE_URL: `postgresql://test:test@${names.db}:5432/container_smoke`,
    REDIS_URL: `redis://${names.redis}:6379`,
    R2_ACCESS_KEY_ID: "test",
    R2_SECRET_ACCESS_KEY: "test",
    R2_ENDPOINT: "http://127.0.0.1:9",
    R2_BUCKET_NAME: "container-smoke",
    R2_PUBLIC_URL: "http://127.0.0.1:9",
    JWT_SECRET: "container-smoke-user-secret",
    ADMIN_JWT_SECRET: "container-smoke-admin-secret",
    ADMIN_EMAIL: "admin@example.com",
    ADMIN_PASSWORD_HASH:
      "$2b$12$R7yqC1s2Q8v7j3W6h8L5uOe4P6VQ3C8m0hD6yQXJbM0Frxmp8A3re",
    APP_BASE_URL: "http://localhost:5175",
    ALLOWED_ORIGINS: "http://localhost:5175",
    ENABLE_ML_PROCESSING: "true",
    ENABLE_ML_WORKER: "false",
    ENABLE_VIRUS_SCAN: "false",
    ENABLE_PENDING_AUTO_APPROVE: "false",
    IGNORE_MISSING_MIGRATIONS: "false",
  };
  docker(
    "run",
    "--detach",
    "--name",
    names.api,
    "--network",
    prefix,
    "--network-alias",
    "backend",
    "-p",
    "127.0.0.1::3000",
    ...Object.entries(config).flatMap(([key, value]) => [
      "-e",
      `${key}=${value}`,
    ]),
    apiImage,
  );
  const api = origin(names.api, 3000);
  console.log(
    "Waiting for API startup, native model loading, and migrations...",
  );
  await waitForHttp(`${api}/health`, apiHealthy);
  const libraries = docker(
    "exec",
    names.api,
    "sh",
    "-c",
    'ldd /usr/local/bin/api && ldd "$ORT_DYLIB_PATH"',
  );
  assert.ok(!libraries.includes("not found"), libraries);
  console.log(docker("exec", names.api, "onnx-smoke"));
  const migrationCount = readdirSync(
    new URL("../apps/api/migrations/", import.meta.url),
  ).filter((name) => name.endsWith(".sql")).length;
  const appliedCount = () =>
    Number(
      docker(
        "exec",
        names.db,
        "psql",
        "-U",
        "test",
        "-d",
        "container_smoke",
        "-Atc",
        "SELECT COUNT(*) FROM _sqlx_migrations WHERE success",
      ),
    );
  assert.equal(
    appliedCount(),
    migrationCount,
    "API did not apply all bundled migrations",
  );

  docker(
    "run",
    "--detach",
    "--name",
    names.web,
    "--network",
    prefix,
    "-p",
    "127.0.0.1::80",
    webImage,
  );
  const web = origin(names.web, 80);
  await waitForHttp(web, (response) => response.ok);
  for (const path of ["/", "/map", "/auth"]) {
    const response = await fetch(`${web}${path}`);
    assert.equal(response.status, 200);
    const html = await response.text();
    assert.match(html, /<div id="root"/);
    const asset = html.match(/src="(\/assets\/[^\"]+\.js)"/);
    assert.ok(asset, "SPA entry script missing");
    const script = await fetch(`${web}${asset[1]}`);
    assert.equal(script.status, 200);
    assert.match(script.headers.get("content-type"), /javascript/);
  }
  for (const path of ["/api/v1/cities", "/api/v1/letterings?limit=1"]) {
    const response = await fetch(`${web}${path}`);
    assert.equal(response.status, 200, path);
    assert.equal(response.headers.get("cache-control"), "no-store");
    const data = await response.json();
    assert.ok(Array.isArray(path.includes("cities") ? data : data.letterings));
  }
  await websocket(`${web.replace("http:", "ws:")}/ws/feed`);
  docker("restart", names.api);
  await waitForHttp(`${api}/health`, apiHealthy);
  assert.equal(
    appliedCount(),
    migrationCount,
    "Restart changed migration history",
  );
  console.log(
    "PASS: native libraries/inference, migration startup/restart, health, SPA/assets, API proxy, and WebSocket upgrade.",
  );
} catch (error) {
  for (const name of [names.api, names.web, names.db]) {
    const logs = spawnSync("docker", ["logs", "--tail", "80", name], {
      encoding: "utf8",
    });
    console.error(`${name}:\n${logs.stdout || ""}${logs.stderr || ""}`);
  }
  throw error;
} finally {
  for (const name of Object.values(names).reverse())
    spawnSync("docker", ["rm", "--force", name], { stdio: "ignore" });
  spawnSync("docker", ["network", "rm", prefix], { stdio: "ignore" });
}
