import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHmac, randomUUID } from "node:crypto";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { setTimeout as delay } from "node:timers/promises";

// Every service and record belongs to this disposable test network.
const prefix = `ttl-smoke-${process.pid}-${Date.now()}`;
const names = Object.fromEntries(
  ["db", "redis", "api", "api2", "restored", "web"].map((key) => [
    key,
    `${prefix}-${key}`,
  ]),
);
const limits = JSON.parse(
  readFileSync(
    new URL("../apps/api/config/runtime.json", import.meta.url),
    "utf8",
  ),
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
    let opened = false;
    const timer = setTimeout(() => {
      socket.close();
      reject(new Error("WebSocket upgrade timed out"));
    }, 10_000);
    socket.addEventListener(
      "open",
      () => {
        opened = true;
        socket.close();
      },
      { once: true },
    );
    socket.addEventListener(
      "close",
      () => {
        clearTimeout(timer);
        if (opened) resolve();
        else reject(new Error("WebSocket closed before upgrade"));
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
    ENABLE_ML_WORKER: "true",
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
  // Docker can allocate a new host port when restarting a dynamic mapping.
  await waitForHttp(`${origin(names.api, 3000)}/health`, apiHealthy);
  assert.equal(
    appliedCount(),
    migrationCount,
    "Restart changed migration history",
  );

  const sql = (query, database = "container_smoke") =>
    docker(
      "exec",
      names.db,
      "psql",
      "-U",
      "test",
      "-d",
      database,
      "-v",
      "ON_ERROR_STOP=1",
      "-Atc",
      query,
    );
  const startReplica = (name, database) =>
    docker(
      "run",
      "--detach",
      "--name",
      name,
      "--network",
      prefix,
      "-p",
      "127.0.0.1::3000",
      ...Object.entries({
        ...config,
        DATABASE_URL: `postgresql://test:test@${names.db}:5432/${database}`,
      }).flatMap(([key, value]) => ["-e", `${key}=${value}`]),
      apiImage,
    );
  startReplica(names.api2, "container_smoke");
  const replicas = [origin(names.api, 3000), origin(names.api2, 3000)];
  await waitForHttp(`${replicas[1]}/health`, apiHealthy);
  const get = async (base, path) => {
    const response = await fetch(`${base}${path}`, {
      signal: AbortSignal.timeout(10_000),
    });
    assert.equal(response.status, 200, path);
    return response.json();
  };
  const header = Buffer.from(
    JSON.stringify({ alg: "HS256", typ: "JWT" }),
  ).toString("base64url");
  const payload = Buffer.from(
    JSON.stringify({
      sub: "container-operator",
      aud: "admin",
      exp: Math.floor(Date.now() / 1000) + 600,
    }),
  ).toString("base64url");
  const unsigned = `${header}.${payload}`;
  const adminToken = `${unsigned}.${createHmac("sha256", config.ADMIN_JWT_SECRET).update(unsigned).digest("base64url")}`;
  const photo = randomUUID();
  for (const base of replicas)
    assert.equal((await get(base, "/api/v1/letterings?limit=1")).total, 0);
  sql(`INSERT INTO letterings(id,city_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,status)
    SELECT '${photo}',id,'Recovery fixture','http://fixture.test/photo','small','medium','large','560001','APPROVED' FROM cities ORDER BY id LIMIT 1`);
  for (const base of replicas)
    assert.equal(
      (await get(base, "/api/v1/letterings?limit=1")).letterings[0].id,
      photo,
    );
  const moderation = await fetch(
    `${replicas[1]}/api/v1/admin/letterings/${photo}/reject`,
    {
      method: "POST",
      headers: {
        authorization: `Bearer ${adminToken}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ reason: "Container recovery check" }),
      signal: AbortSignal.timeout(10_000),
    },
  );
  assert.equal(moderation.status, 204);
  for (const base of replicas)
    assert.equal((await get(base, "/api/v1/letterings?limit=1")).total, 0);
  await Promise.all(
    Array.from({ length: 30 }, (_, index) =>
      get(
        replicas[index % 2],
        index % 3 ? "/api/v1/letterings?limit=1" : "/health",
      ),
    ),
  );

  // INFO is the observer's only command. Retain all application setup and Lua subcommands.
  const commandCounts = () =>
    Object.fromEntries(
      docker("exec", names.redis, "redis-cli", "--raw", "INFO", "commandstats")
        .split(/\r?\n/)
        .flatMap((line) => {
          const match = line.match(/^cmdstat_(.+):calls=(\d+)/);
          return match ? [[match[1], Number(match[2])]] : [];
        }),
    );
  const delta = (before, after) =>
    Object.fromEntries(
      [...new Set([...Object.keys(before), ...Object.keys(after)])]
        .map((key) => [key, (after[key] || 0) - (before[key] || 0)])
        .filter(([, count]) => count),
    );
  const observations = [];
  const measure = async (name, action) => {
    const before = commandCounts();
    await action();
    const commands = delta(before, commandCounts());
    // The first snapshot's INFO is counted in the second snapshot.
    // Subtract that observation, preserving any INFO sent by the application.
    commands.info = (commands.info || 0) - 1;
    if (commands.info === 0) delete commands.info;
    observations.push({
      name,
      commands,
      total: Object.values(commands).reduce((sum, count) => sum + count, 0),
    });
    return commands;
  };
  const idleSeconds = limits.queue.poll_seconds + 1;
  console.log(
    `Observing ${idleSeconds}s of idle operation with two enabled analysis workers...`,
  );
  const idle = await measure("idle workers and warm public reads", async () => {
    await delay(idleSeconds * 1000);
    for (const base of replicas) await get(base, "/api/v1/letterings?limit=1");
  });
  assert.deepEqual(
    idle,
    {},
    "Idle workers and public reads must not issue Redis commands",
  );
  const login = (base) =>
    fetch(`${base}/api/v1/admin/login`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        email: "unknown@example.invalid",
        password: "fixture",
      }),
      signal: AbortSignal.timeout(10_000),
    });
  await measure("first protected attempt and connection setup", async () =>
    assert.equal((await login(replicas[0])).status, 403),
  );
  const allowed = limits.security.admin_login.requests;
  await measure("remaining admitted attempts", async () => {
    for (let index = 1; index < allowed; index++)
      assert.equal((await login(replicas[0])).status, 403);
  });
  await measure("exhausted quota", async () => {
    const result = await login(replicas[0]);
    assert.equal(result.status, 429);
    assert.ok(Number(result.headers.get("retry-after")) > 0);
  });
  await measure("second replica connection and shared quota", async () =>
    assert.equal((await login(replicas[1])).status, 429),
  );

  docker("pause", names.redis);
  try {
    for (const base of replicas) {
      await get(base, "/health");
      await get(base, "/api/v1/letterings?limit=1");
    }
    assert.equal(
      (await login(replicas[0])).status,
      503,
      "Protected actions must fail closed during a Redis outage",
    );
  } finally {
    docker("unpause", names.redis);
  }
  const resumed = await login(replicas[0]);
  assert.equal(
    resumed.status,
    429,
    "The connection and existing quota must survive recovery",
  );

  sql("DELETE FROM public_cache_revision");
  for (const base of replicas) {
    const health = await fetch(`${base}/health`, {
      signal: AbortSignal.timeout(10_000),
    });
    assert.equal(health.status, 503);
    const status = await health.json();
    assert.equal(status.status, "unhealthy");
    assert.equal(status.database, "up");
    assert.equal(status.cache_consistency, "down");
    assert.equal(
      (
        await fetch(`${base}/api/v1/letterings?limit=1`, {
          signal: AbortSignal.timeout(10_000),
        })
      ).status,
      503,
    );
  }
  sql("INSERT INTO public_cache_revision(singleton) VALUES(true)");
  for (const base of replicas) await get(base, "/health");
  const epoch = sql("SELECT epoch FROM public_cache_revision");
  docker(
    "exec",
    names.db,
    "pg_dump",
    "-U",
    "test",
    "-d",
    "container_smoke",
    "-Fc",
    "-f",
    "/tmp/recovery.dump",
  );
  docker("exec", names.db, "createdb", "-U", "test", "restored_smoke");
  docker(
    "exec",
    names.db,
    "pg_restore",
    "--exit-on-error",
    "--no-owner",
    "-U",
    "test",
    "-d",
    "restored_smoke",
    "/tmp/recovery.dump",
  );
  sql(
    "UPDATE public_cache_revision SET epoch=gen_random_uuid() WHERE singleton",
    "restored_smoke",
  );
  assert.notEqual(
    sql("SELECT epoch FROM public_cache_revision", "restored_smoke"),
    epoch,
  );
  assert.equal(
    sql(`SELECT status FROM letterings WHERE id='${photo}'`, "restored_smoke"),
    "REJECTED",
  );
  startReplica(names.restored, "restored_smoke");
  const restored = origin(names.restored, 3000);
  await waitForHttp(`${restored}/health`, apiHealthy);
  assert.equal((await get(restored, "/api/v1/letterings?limit=1")).total, 0);
  assert.equal(
    Number(
      sql(
        "SELECT COUNT(*) FROM _sqlx_migrations WHERE success",
        "restored_smoke",
      ),
    ),
    migrationCount,
  );
  const report = {
    scope: "disposable_containers",
    api_image: apiImage,
    web_image: webImage,
    idle_seconds: idleSeconds,
    active_replicas: 2,
    protected_attempts: allowed + 2,
    observations,
    restored_database: true,
    rotated_restore_epoch: true,
    shared_moderation_visibility: true,
    redis_outage_fails_closed: true,
    missing_freshness_fails_closed: true,
  };
  console.log(JSON.stringify(report, null, 2));
  if (process.env.TTL_OPERATIONS_REPORT)
    writeFileSync(
      process.env.TTL_OPERATIONS_REPORT,
      JSON.stringify(report, null, 2) + "\n",
    );
  console.log(
    "PASS: native inference, startup/restart, routes/assets, proxy, WebSocket, replicas, command counts, dependency faults, and database restore.",
  );
} catch (error) {
  for (const name of [
    names.api,
    names.api2,
    names.restored,
    names.web,
    names.db,
  ]) {
    const logs = spawnSync("docker", ["logs", "--tail", "80", name], {
      encoding: "utf8",
    });
    console.error(`${name}:\n${logs.stdout || ""}${logs.stderr || ""}`);
  }
  throw error;
} finally {
  for (const name of Object.values(names).reverse())
    spawnSync("docker", ["rm", "--force", "--volumes", name], {
      stdio: "ignore",
    });
  spawnSync("docker", ["network", "rm", prefix], { stdio: "ignore" });
}
