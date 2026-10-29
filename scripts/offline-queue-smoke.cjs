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
const transpile = (source) =>
  ts.transpileModule(source, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.ESNext,
    },
  }).outputText;
const sessionSource = transpile(
  fs.readFileSync(path.join(root, "apps/web/src/lib/userSession.ts"), "utf8"),
);
function moduleSource(url) {
  const backend = url.searchParams.get("backend");
  const mode = url.searchParams.get("mode");
  const source = queueSource
    .replace('import { API_BASE_URL } from "../constants";', "")
    .replace('from "./userSession"', 'from "/session.js"')
    .replace('from "./api"', 'from "/session.js"')
    .replace('import { offlineDatabaseName } from "./offlineNamespace";', "")
    .replaceAll("import.meta.env.VITE_OFFLINE_BACKEND", JSON.stringify(backend))
    .replaceAll("import.meta.env.MODE", JSON.stringify(mode));
  return transpile(
    `${namespaceSource}\nconst API_BASE_URL=${JSON.stringify(backend)};\n${source}\nexport { offlineDatabaseName };`,
  );
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
      url.pathname.endsWith(".js") ? "text/javascript" : "text/html",
    );
    res.end(
      url.pathname === "/queue.js"
        ? moduleSource(url)
        : url.pathname === "/session.js"
          ? sessionSource
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
    const credentials = [];
    await context.route("https://*-api.test/**", (route) => {
      uploads.push(route.request().url());
      credentials.push(route.request().headers().authorization);
      return route.fulfill({ json: { id: "fixture" } });
    });
    const page = await context.newPage();
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    const result = await page.evaluate(
      async ({ localBackend, liveBackend }) => {
        const session = await import("/session.js");
        session.replaceUserSession("account-a-token", {
          id: "a",
          email: "a@example.com",
          role: "USER",
          created_at: "2026-01-01",
        });
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
        const unclaimedName = local.offlineDatabaseName(
          localBackend,
          "development",
          location.origin,
        );
        const unclaimed = await new Promise((resolve, reject) => {
          const req = indexedDB.open(unclaimedName, 1);
          req.onupgradeneeded = () =>
            req.result.createObjectStore("uploads", {
              keyPath: "id",
              autoIncrement: true,
            });
          req.onsuccess = () => resolve(req.result);
          req.onerror = () => reject(req.error);
        });
        await new Promise((resolve, reject) => {
          const tx = unclaimed.transaction("uploads", "readwrite");
          tx.objectStore("uploads").add(record);
          tx.oncomplete = resolve;
          tx.onerror = () => reject(tx.error);
        });
        unclaimed.close();
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
    assert.deepEqual(credentials, [
      "Bearer account-a-token",
      "Bearer account-a-token",
    ]);
    const accounts = await page.evaluate(async (backend) => {
      const session = await import("/session.js");
      const queue = await import(
        `/queue.js?backend=${encodeURIComponent(backend)}&mode=development`
      );
      const user = (id) => ({
        id,
        email: `${id}@example.com`,
        role: "USER",
        created_at: "2026-01-01",
      });
      const record = {
        imageBlob: new Blob(["private-photo"]),
        fileName: "a.png",
        contributorTag: "A",
        pinCode: "560001",
        description: "A private draft",
        cityId: "city",
      };
      await queue.enqueueUpload(record);
      session.replaceUserSession(null, null);
      const loggedOut = await queue.getQueuedCount();
      const loggedOutSync = await queue.syncOfflineUploads();
      let rejected = false;
      try {
        await queue.enqueueUpload(record);
      } catch {
        rejected = true;
      }
      session.replaceUserSession("account-b-token", user("b"));
      const other = await queue.getQueuedCount();
      const otherSync = await queue.syncOfflineUploads();
      await queue.enqueueUpload({
        ...record,
        fileName: "b.png",
        description: "B draft",
      });
      const ownSync = await queue.syncOfflineUploads();
      session.replaceUserSession("account-a-new-token", user("a"));
      const recovered = await queue.getQueuedCount();
      const db = await new Promise((resolve, reject) => {
        const req = indexedDB.open(
          queue.offlineDatabaseName(backend, "development", location.origin),
          2,
        );
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => reject(req.error);
      });
      const rows = await new Promise((resolve, reject) => {
        const req = db.transaction("uploads").objectStore("uploads").getAll();
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => reject(req.error);
      });
      db.close();
      return {
        loggedOut,
        loggedOutSync,
        rejected,
        other,
        otherSync,
        ownSync,
        recovered,
        unclaimed: rows.filter((row) => !row.ownerId).length,
        blobs: rows.every((row) => row.imageBlob.size > 0),
        storedToken: rows.some((row) => "token" in row),
      };
    }, localBackend);
    assert.deepEqual(accounts, {
      loggedOut: 0,
      loggedOutSync: 0,
      rejected: true,
      other: 0,
      otherSync: 0,
      ownSync: 1,
      recovered: 1,
      unclaimed: 1,
      blobs: true,
      storedToken: false,
    });
    assert.equal(credentials.at(-1), "Bearer account-b-token");

    let release;
    let observed;
    const started = new Promise((resolve) => {
      observed = resolve;
    });
    await context.route(`${localBackend}/**`, async (route) => {
      observed();
      await new Promise((resolve) => {
        release = resolve;
      });
      await route.fulfill({ json: { id: "slow" } }).catch(() => {});
    });
    await page.evaluate(async (backend) => {
      const queue = await import(
        `/queue.js?backend=${encodeURIComponent(backend)}&mode=development`
      );
      window.replay = queue.syncOfflineUploads();
    }, localBackend);
    await started;
    const switched = await page.evaluate(async () => {
      const session = await import("/session.js");
      session.replaceUserSession("account-b-token", {
        id: "b",
        email: "b@example.com",
        role: "USER",
        created_at: "2026-01-01",
      });
      return await window.replay;
    });
    release();
    assert.equal(switched, 0);
    const retained = await page.evaluate(async (backend) => {
      const session = await import("/session.js");
      session.replaceUserSession("account-a-token", {
        id: "a",
        email: "a@example.com",
        role: "USER",
        created_at: "2026-01-01",
      });
      return (
        await import(
          `/queue.js?backend=${encodeURIComponent(backend)}&mode=development`
        )
      ).getQueuedCount();
    }, localBackend);
    assert.equal(retained, 1);
    console.log(
      "PASS: backend/mode and account isolation, legacy quarantine, retained blobs, reload, logout/login, and account change during replay",
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
