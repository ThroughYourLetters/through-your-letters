// Isolated browser fixture; no requests reach real backends.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const http = require("node:http");
const ts = require("typescript");
const { chromium } = require("playwright");
const root = path.resolve(__dirname, "..");
const queueSource = fs.readFileSync(
  path.join(root, "apps/web/src/lib/offlineQueue.ts"),
  "utf8",
);
const namespaceSource = fs
  .readFileSync(path.join(root, "apps/web/src/lib/offlineNamespace.ts"), "utf8")
  .replace("export function", "function");
const localBackend = "https://local-api.test";
const liveBackend = "https://live-api.test";
function moduleSource(url) {
  const backend = url.searchParams.get("backend");
  const mode = url.searchParams.get("mode");
  const source = queueSource
    .replace('import { API_BASE_URL } from "../constants";', "")
    .replace('import { USER_SESSION_KEY } from "./api";', "")
    .replace('import { offlineDatabaseName } from "./offlineNamespace";', "")
    .replaceAll("import.meta.env.VITE_OFFLINE_BACKEND", JSON.stringify(backend))
    .replaceAll("import.meta.env.MODE", JSON.stringify(mode));
  return ts.transpileModule(
    `${namespaceSource}\nconst API_BASE_URL=${JSON.stringify(backend)};\nconst USER_SESSION_KEY='ttl_user_token';\n${source}`,
    {
      compilerOptions: {
        target: ts.ScriptTarget.ES2022,
        module: ts.ModuleKind.ESNext,
      },
    },
  ).outputText;
}
(async () => {
  const { loadConfigFromFile } =
    await import("../apps/web/node_modules/vite/dist/node/index.js");
  const previousDirectory = process.cwd();
  try {
    process.chdir(path.join(root, "apps/web"));
    const local = await loadConfigFromFile({
      command: "serve",
      mode: "development",
    });
    const live = await loadConfigFromFile({ command: "serve", mode: "live" });
    assert.notEqual(local.config.server.port, live.config.server.port);
    assert.equal(local.config.server.strictPort, true);
    assert.equal(live.config.server.strictPort, true);
  } finally {
    process.chdir(previousDirectory);
  }
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, "http://fixture.test");
    res.setHeader(
      "Content-Type",
      url.pathname === "/queue.js" ? "text/javascript" : "text/html",
    );
    res.end(
      url.pathname === "/queue.js"
        ? moduleSource(url)
        : "<!doctype html><title>Offline queue fixture</title>",
    );
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const browser = await chromium.launch({
    headless: true,
    channel: "chromium",
  });
  try {
    const context = await browser.newContext();
    const uploads = [];
    await context.route("https://*-api.test/**", (route) => {
      uploads.push(route.request().url());
      return route.fulfill({ json: { id: "fixture" } });
    });
    const page = await context.newPage();
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    const result = await page.evaluate(
      async ({ localBackend, liveBackend }) => {
        const load = (backend, mode) =>
          import(
            `/queue.js?backend=${encodeURIComponent(backend)}&mode=${mode}`
          );
        const record = {
          imageBlob: new Blob(["fixture"], { type: "image/png" }),
          fileName: "sign.png",
          contributorTag: "Tester",
          pinCode: "560001",
          description: "Fixture",
          cityId: "same-city-id",
        };
        const legacy = await new Promise((resolve, reject) => {
          const req = indexedDB.open("tyl-offline", 1);
          req.onupgradeneeded = () =>
            req.result.createObjectStore("uploads", {
              keyPath: "id",
              autoIncrement: true,
            });
          req.onsuccess = () => resolve(req.result);
          req.onerror = () => reject(req.error);
        });
        await new Promise((resolve, reject) => {
          const tx = legacy.transaction("uploads", "readwrite");
          tx.objectStore("uploads").add(record);
          tx.oncomplete = resolve;
          tx.onerror = () => reject(tx.error);
        });
        const local = await load(localBackend, "development");
        await local.enqueueUpload(record);
        const live = await load(liveBackend, "live");
        const sameModeOtherBackend = await load(liveBackend, "development");
        const sameBackendOtherMode = await load(localBackend, "live");
        const isolated = [
          await live.getQueuedCount(),
          await sameModeOtherBackend.getQueuedCount(),
          await sameBackendOtherMode.getQueuedCount(),
        ];
        const emptySync = await live.syncOfflineUploads();
        await live.enqueueUpload(record);
        const liveSynced = await live.syncOfflineUploads();
        const localRemaining = await local.getQueuedCount();
        const legacyRemaining = await new Promise((resolve, reject) => {
          const req = legacy
            .transaction("uploads", "readonly")
            .objectStore("uploads")
            .count();
          req.onsuccess = () => resolve(req.result);
          req.onerror = () => reject(req.error);
        });
        legacy.close();
        return {
          isolated,
          emptySync,
          liveSynced,
          localRemaining,
          legacyRemaining,
        };
      },
      { localBackend, liveBackend },
    );
    assert.deepEqual(result, {
      isolated: [0, 0, 0],
      emptySync: 0,
      liveSynced: 1,
      localRemaining: 1,
      legacyRemaining: 1,
    });
    assert.deepEqual(uploads, [`${liveBackend}/api/v1/letterings/upload`]);
    await page.reload();
    const localSynced = await page.evaluate(async (backend) => {
      const queue = await import(
        `/queue.js?backend=${encodeURIComponent(backend)}&mode=development`
      );
      return {
        before: await queue.getQueuedCount(),
        synced: await queue.syncOfflineUploads(),
        after: await queue.getQueuedCount(),
      };
    }, localBackend);
    assert.deepEqual(localSynced, { before: 1, synced: 1, after: 0 });
    assert.deepEqual(uploads, [
      `${liveBackend}/api/v1/letterings/upload`,
      `${localBackend}/api/v1/letterings/upload`,
    ]);
    console.log(
      "PASS: same-origin backend/mode isolation, matching-destination replay, retained local/legacy uploads, persistence after reload",
    );
    await context.close();
  } finally {
    await browser.close();
    await new Promise((resolve) => server.close(resolve));
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
