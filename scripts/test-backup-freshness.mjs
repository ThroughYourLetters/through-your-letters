import assert from "node:assert/strict";
import { once } from "node:events";
import { createServer } from "node:http";
import { after, test } from "node:test";
import {
  checkBackupFreshness,
  pruneDatabaseBackups,
} from "./check-backup-freshness.mjs";

const now = new Date("2026-11-22T12:00:00Z");
const token = "fixture-only-secret";
let replies;
let observed;
const methods = [];
const server = createServer((request, response) => {
  methods.push(request.method);
  observed.push({ url: request.url, auth: request.headers.authorization });
  const reply = replies.shift();
  assert.ok(reply, "unexpected request");
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
  assert.equal(new URL(url).origin, "https://api.github.com");
  assert.equal(options.redirect, "error");
  return fetch(
    `http://127.0.0.1:${server.address().port}${new URL(url).pathname}${new URL(url).search}`,
    options,
  );
};
const check = () =>
  checkBackupFreshness({ repository: "owner/repo", token, now, request });
const run = {
  id: 42,
  head_branch: "main",
  conclusion: "success",
  event: "schedule",
};
const artifact = {
  name: "database-backup-42-1",
  expired: false,
  size_in_bytes: 2000,
  created_at: "2026-11-22T02:43:00Z",
  expires_at: "2026-11-29T02:43:00Z",
};
function setup(body = artifact) {
  methods.length = 0;
  observed = [];
  replies = [
    { body: { workflow_runs: [run] } },
    { body: { artifacts: [body] } },
  ];
}

test("checks a successful main-branch backup and its retained artifact through HTTP", async () => {
  setup();
  const result = await check();
  assert.equal(result.backup_run_id, 42);
  assert.ok(result.age_hours > 9 && result.age_hours < 10);
  assert.deepEqual(observed, [
    {
      url: "/repos/owner/repo/actions/workflows/database-backup.yml/runs?branch=main&status=success&per_page=20",
      auth: `Bearer ${token}`,
    },
    {
      url: "/repos/owner/repo/actions/runs/42/artifacts?per_page=100",
      auth: `Bearer ${token}`,
    },
  ]);
});

test("expired, missing, stale, empty, and future artifacts fail", async () => {
  for (const change of [
    { expired: true },
    { size_in_bytes: 0 },
    { name: "http-observations" },
    { created_at: "2026-11-21T05:00:00Z" },
    { expires_at: "2026-11-22T01:00:00Z" },
    { created_at: "2026-11-23T00:00:00Z" },
  ]) {
    setup({ ...artifact, ...change });
    await assert.rejects(check(), /No successful retained/);
  }
  setup();
  replies[1].body.artifacts = [];
  await assert.rejects(check(), /No successful retained/);
});

test("pull requests and non-main branches cannot satisfy production backup monitoring", async () => {
  setup();
  replies[0].body.workflow_runs = [
    { ...run, event: "pull_request" },
    { ...run, head_branch: "feature" },
  ];
  await assert.rejects(check(), /No successful retained/);
  assert.equal(observed.length, 1);
});

test("an older retained successful run remains usable after a newer run has no artifact", async () => {
  setup();
  replies[0].body.workflow_runs.unshift({ ...run, id: 43 });
  replies.splice(1, 0, { body: { artifacts: [] } });
  assert.equal((await check()).backup_run_id, 42);
});

test("retention confirms the new upload and deletes only older backup artifacts", async () => {
  setup();
  const backups = Array.from({ length: 10 }, (_, index) => ({
    ...artifact,
    id: index + 1,
    name: `database-backup-${index + 1}-1`,
    created_at: new Date(now.getTime() + index * 1000).toISOString(),
  }));
  replies = [
    { body: backups[9] },
    {
      body: {
        artifacts: [
          ...backups,
          { ...backups[0], id: 99, name: "operations-observation" },
        ],
      },
    },
    ...Array.from({ length: 3 }, () => ({ status: 204, body: "" })),
  ];
  assert.deepEqual(
    await pruneDatabaseBackups({
      repository: "owner/repo",
      token,
      currentArtifactId: 10,
      request,
    }),
    { retained: 7, deleted: 3 },
  );
  assert.deepEqual(methods, ["GET", "GET", "DELETE", "DELETE", "DELETE"]);
  assert.deepEqual(
    observed.slice(2).map((item) => item.url),
    [
      "/repos/owner/repo/actions/artifacts/3",
      "/repos/owner/repo/actions/artifacts/2",
      "/repos/owner/repo/actions/artifacts/1",
    ],
  );
});

test("missing new uploads and invalid listings cannot delete retained backups", async () => {
  setup();
  replies = [{ body: { ...artifact, id: 10, expired: true } }];
  await assert.rejects(
    pruneDatabaseBackups({
      repository: "owner/repo",
      token,
      currentArtifactId: 10,
      request,
    }),
    /must exist before/,
  );
  assert.ok(methods.every((method) => method === "GET"));
  setup();
  replies = [{ body: { ...artifact, id: 10 } }, { body: { artifacts: [] } }];
  await assert.rejects(
    pruneDatabaseBackups({
      repository: "owner/repo",
      token,
      currentArtifactId: 10,
      request,
    }),
    /nothing was deleted/,
  );
  assert.ok(methods.every((method) => method === "GET"));
});

test("API failures and malformed responses fail without exposing provider messages", async () => {
  for (const status of [401, 403, 429, 503]) {
    setup();
    replies[0] = { status, body: token };
    await assert.rejects(check(), {
      message: `Backup monitoring returned HTTP ${status}`,
    });
  }
  setup();
  replies[0].body = "not-json";
  await assert.rejects(check(), /not JSON/);
  setup();
  replies[0].body = {};
  await assert.rejects(check(), /no run list/);
  await assert.rejects(
    checkBackupFreshness({
      repository: "owner/repo",
      token,
      now,
      request: async () => {
        throw new Error(token);
      },
    }),
    { message: "Backup monitoring request failed or timed out" },
  );
});
