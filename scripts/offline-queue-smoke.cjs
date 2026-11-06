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
const runtime = require("../apps/api/config/runtime.json");
const runtimeSource = `export const webLimits=${JSON.stringify(runtime.web)}; export const uploadTimeoutMs=${runtime.images.upload_timeout_ms};`;
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
    .replace('from "./runtimeConfig"', 'from "/runtime.js"')
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
          : url.pathname === "/runtime.js"
            ? runtimeSource
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

    await context.unroute(`${localBackend}/**`);
    // Simulate an account-owned v2 record saved before request keys existed.
    await page.evaluate(async (backend) => {
      const queue = await import(
        `/queue.js?backend=${encodeURIComponent(backend)}&mode=development`
      );
      const db = await new Promise((resolve, reject) => {
        const request = indexedDB.open(
          queue.offlineDatabaseName(backend, "development", location.origin),
        );
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
      await new Promise((resolve, reject) => {
        const tx = db.transaction("uploads", "readwrite");
        const store = tx.objectStore("uploads");
        const request = store.getAll();
        request.onsuccess = () => {
          const row = request.result.find((row) => row.ownerId === "a");
          delete row.requestKey;
          store.put(row);
        };
        tx.oncomplete = resolve;
        tx.onerror = () => reject(tx.error);
      });
      db.close();
    }, localBackend);
    const retryKeys = [];
    const accepted = new Set();
    await context.route(`${localBackend}/**`, async (route) => {
      const key = route.request().headers()["idempotency-key"];
      assert.match(key, /^[0-9a-f-]{36}$/);
      assert.ok(
        route.request().postDataBuffer().includes(Buffer.from("private-photo")),
      );
      retryKeys.push(key);
      accepted.add(key);
      if (retryKeys.length === 1) await route.abort("connectionfailed");
      else
        await route.fulfill({
          json: { id: "original-upload", status: "approved" },
        });
    });
    const sync = (backend) =>
      page.evaluate(async (backend) => {
        const queue = await import(
          `/queue.js?backend=${encodeURIComponent(backend)}&mode=development`
        );
        return {
          synced: await queue.syncOfflineUploads(),
          remaining: await queue.getQueuedCount(),
        };
      }, backend);
    assert.deepEqual(await sync(localBackend), { synced: 0, remaining: 1 });
    await page.reload();
    assert.deepEqual(await sync(localBackend), { synced: 1, remaining: 0 });
    assert.equal(retryKeys.length, 2);
    assert.equal(retryKeys[0], retryKeys[1]);
    assert.equal(accepted.size, 1);

    const peer = await context.newPage();
    await peer.goto(`http://127.0.0.1:${server.address().port}`);
    const setAccount = (tab, id) =>
      tab.evaluate(async (id) => {
        const session = await import("/session.js");
        session.replaceUserSession(`account-${id}-token`, {
          id,
          email: `${id}@example.com`,
          role: "USER",
          created_at: "2026-01-01",
        });
      }, id);
    const startReplay = (tab, enqueue) =>
      tab.evaluate(
        async ({ backend, enqueue }) => {
          const queue = await import(
            `/queue.js?backend=${encodeURIComponent(backend)}&mode=development`
          );
          if (enqueue)
            await queue.enqueueUpload({
              imageBlob: new Blob(["shared-photo"]),
              fileName: "shared.png",
              contributorTag: "A",
              pinCode: "560001",
              description: "Cross-tab draft",
              cityId: "city",
            });
          window.tabReplay = queue.syncOfflineUploads();
        },
        { backend: localBackend, enqueue },
      );
    const finishReplay = (tab) => tab.evaluate(() => window.tabReplay);
    const waitForPeer = () =>
      peer.waitForFunction(async () =>
        (await navigator.locks.query()).pending.some((lock) =>
          lock.name.startsWith("ttl-offline-replay:"),
        ),
      );
    await setAccount(peer, "a");
    await context.unroute(`${localBackend}/**`);
    let tabRelease;
    let tabObserved;
    let tabCalls = 0;
    let ownerStarted = new Promise((resolve) => {
      tabObserved = resolve;
    });
    await context.route(`${localBackend}/**`, async (route) => {
      tabCalls++;
      tabObserved();
      await new Promise((resolve) => {
        tabRelease = resolve;
      });
      await route.fulfill({ json: { id: "shared" } }).catch(() => {});
    });
    await startReplay(page, true);
    await ownerStarted;
    await startReplay(peer, false);
    await waitForPeer();
    assert.equal(tabCalls, 1);
    tabRelease();
    assert.deepEqual(
      await Promise.all([finishReplay(page), finishReplay(peer)]),
      [1, 0],
    );

    // A tab changing accounts while queued must cancel its old lock request.
    ownerStarted = new Promise((resolve) => {
      tabObserved = resolve;
    });
    await startReplay(page, true);
    await ownerStarted;
    await startReplay(peer, false);
    await waitForPeer();
    await setAccount(peer, "b");
    assert.equal(await finishReplay(peer), 0);
    assert.equal(tabCalls, 2);
    tabRelease();
    assert.equal(await finishReplay(page), 1);

    // Closing the lock owner releases the lock; the next tab keeps the same request key.
    await setAccount(peer, "a");
    await context.unroute(`${localBackend}/**`);
    const takeoverKeys = [];
    let releaseClosedOwner;
    const closedOwnerStarted = new Promise((resolve) => {
      tabObserved = resolve;
    });
    await context.route(`${localBackend}/**`, async (route) => {
      takeoverKeys.push(route.request().headers()["idempotency-key"]);
      if (takeoverKeys.length === 1) {
        tabObserved();
        await new Promise((resolve) => {
          releaseClosedOwner = resolve;
        });
      }
      await route
        .fulfill({ json: { id: "original-after-close" } })
        .catch(() => {});
    });
    await startReplay(page, true);
    await closedOwnerStarted;
    await startReplay(peer, false);
    await waitForPeer();
    await page.close();
    assert.equal(await finishReplay(peer), 1);
    releaseClosedOwner();
    assert.equal(takeoverKeys.length, 2);
    assert.equal(takeoverKeys[0], takeoverKeys[1]);

    // Browsers without Web Locks still coalesce calls within a tab.
    await context.unroute(`${localBackend}/**`);
    let fallbackCalls = 0;
    await context.route(`${localBackend}/**`, async (route) => {
      fallbackCalls++;
      await route.fulfill({ json: { id: "fallback" } });
    });
    const fallback = await peer.evaluate(async (backend) => {
      Object.defineProperty(navigator, "locks", {
        configurable: true,
        value: undefined,
      });
      const queue = await import(
        `/queue.js?backend=${encodeURIComponent(backend)}&mode=development`
      );
      await queue.enqueueUpload({
        imageBlob: new Blob(["fallback"]),
        fileName: "fallback.png",
        contributorTag: "A",
        pinCode: "560001",
        description: "Fallback",
        cityId: "city",
      });
      return Promise.all([
        queue.syncOfflineUploads(),
        queue.syncOfflineUploads(),
      ]);
    }, localBackend);
    assert.deepEqual(fallback, [1, 0]);
    assert.equal(fallbackCalls, 1);

    await peer.evaluate(async (backend) => {
      window.fixtureQueue = await import(
        `/queue.js?backend=${encodeURIComponent(backend)}&mode=development`
      );
      window.fixtureRecord = {
        imageBlob: new Blob(["retained-private-photo"]),
        fileName: "retained.png",
        contributorTag: "A",
        pinCode: "560001",
        description: "Private draft",
        cityId: "city",
      };
    }, localBackend);
    await context.unroute(`${localBackend}/**`);
    let responseStatus = 400;
    let errorCalls = 0;
    const errorKeys = [];
    await context.route(`${localBackend}/**`, (route) => {
      errorCalls++;
      errorKeys.push(route.request().headers()["idempotency-key"]);
      return route.fulfill({
        status: responseStatus,
        headers: {
          "Retry-After": "60",
          "Access-Control-Expose-Headers": "Retry-After",
        },
        json: {},
      });
    });
    const failed = await peer.evaluate(async () => {
      const queue = window.fixtureQueue;
      await queue.enqueueUpload(window.fixtureRecord);
      await queue.syncOfflineUploads();
      const row = (await queue.getSavedUploads()).items[0];
      return {
        row,
        bytes: await (await queue.getSavedPhoto(row.id)).blob.text(),
        repeat: await queue.syncOfflineUploads(),
      };
    });
    assert.equal(failed.row.state, "failed");
    assert.equal(failed.row.issue, "invalid");
    assert.equal(failed.bytes, "retained-private-photo");
    assert.equal(failed.repeat, 0);
    assert.equal(
      errorCalls,
      1,
      "permanent failures must not replay automatically",
    );
    responseStatus = 200;
    assert.equal(
      await peer.evaluate(
        (id) => window.fixtureQueue.retrySavedUpload(id),
        failed.row.id,
      ),
      1,
    );
    assert.equal(errorKeys[0], errorKeys[1]);

    responseStatus = 429;
    const limited = await peer.evaluate(async () => {
      const queue = window.fixtureQueue;
      await queue.enqueueUpload(window.fixtureRecord);
      await queue.enqueueUpload({
        ...window.fixtureRecord,
        fileName: "second.png",
      });
      await queue.syncOfflineUploads();
      const rows = (await queue.getSavedUploads()).items;
      const again = await queue.syncOfflineUploads();
      let refused = false;
      try {
        await queue.retrySavedUpload(rows[0].id);
      } catch {
        refused = true;
      }
      return { rows, again, refused };
    });
    assert.equal(limited.rows[0].issue, "rate_limit");
    assert.equal(limited.rows[0].state, "blocked");
    assert.ok(limited.rows[0].retryAfter > Date.now() + 50_000);
    assert.equal(limited.again, 0);
    assert.equal(limited.refused, true);
    assert.equal(
      errorCalls,
      3,
      "a cooldown must also block other saved uploads",
    );
    await peer.evaluate(
      async (ids) => {
        for (const id of ids) await window.fixtureQueue.discardSavedUpload(id);
      },
      limited.rows.map((row) => row.id),
    );

    responseStatus = 401;
    const expired = await peer.evaluate(async () => {
      const queue = window.fixtureQueue;
      await queue.enqueueUpload(window.fixtureRecord);
      const id = (await queue.getSavedUploads()).items[0].id;
      await queue.syncOfflineUploads();
      return {
        id,
        token: sessionStorage.getItem("ttl_user_token"),
        count: await queue.getQueuedCount(),
      };
    });
    assert.equal(expired.token, null);
    assert.equal(expired.count, 0);
    await setAccount(peer, "b");
    assert.equal(
      await peer.evaluate(
        (id) => window.fixtureQueue.getSavedPhoto(id),
        expired.id,
      ),
      null,
    );
    await peer.evaluate(
      (id) => window.fixtureQueue.discardSavedUpload(id),
      expired.id,
    );
    await setAccount(peer, "a");
    assert.equal(
      await peer.evaluate(
        async (id) => (await window.fixtureQueue.getSavedPhoto(id)).blob.text(),
        expired.id,
      ),
      "retained-private-photo",
    );
    responseStatus = 200;
    assert.equal(
      await peer.evaluate(() => window.fixtureQueue.syncOfflineUploads()),
      1,
    );

    // An expired fallback claim may be replaced; its late response cannot delete the new claim.
    await context.unroute(`${localBackend}/**`);
    let releaseExpired;
    let claimObserved;
    const claimStarted = new Promise((resolve) => {
      claimObserved = resolve;
    });
    const claimKeys = [];
    await context.route(`${localBackend}/**`, async (route) => {
      claimKeys.push(route.request().headers()["idempotency-key"]);
      if (claimKeys.length === 1) {
        claimObserved();
        await new Promise((resolve) => {
          releaseExpired = resolve;
        });
      }
      await route.fulfill({ json: {} });
    });
    await peer.evaluate(async () => {
      await window.fixtureQueue.enqueueUpload(window.fixtureRecord);
      window.heldReplay = window.fixtureQueue.syncOfflineUploads();
    });
    await claimStarted;
    const active = await peer.evaluate(async () => {
      const queue = window.fixtureQueue;
      const row = (await queue.getSavedUploads()).items[0];
      let refused = false;
      try {
        await queue.discardSavedUpload(row.id);
      } catch {
        refused = true;
      }
      return { state: row.state, refused };
    });
    assert.deepEqual(active, { state: "retrying", refused: true });
    const replacement = await context.newPage();
    await replacement.goto(`http://127.0.0.1:${server.address().port}`);
    await setAccount(replacement, "a");
    const replaceClaim = await replacement.evaluate(async (backend) => {
      Object.defineProperty(navigator, "locks", {
        configurable: true,
        value: undefined,
      });
      const queue = await import(
        `/queue.js?backend=${encodeURIComponent(backend)}&mode=development`
      );
      const before = await queue.syncOfflineUploads();
      const request = indexedDB.open(
        queue.offlineDatabaseName(backend, "development", location.origin),
      );
      const db = await new Promise((resolve, reject) => {
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
      await new Promise((resolve, reject) => {
        const tx = db.transaction("uploads", "readwrite");
        const store = tx.objectStore("uploads");
        const request = store.index("ownerId").getAll("a");
        request.onsuccess = () => {
          const row = request.result[0];
          row.lease.until = Date.now() - 1;
          store.put(row);
        };
        tx.oncomplete = resolve;
        tx.onabort = () => reject(tx.error);
      });
      db.close();
      return { before, after: await queue.syncOfflineUploads() };
    }, localBackend);
    assert.deepEqual(replaceClaim, { before: 0, after: 1 });
    releaseExpired();
    assert.equal(await peer.evaluate(() => window.heldReplay), 0);
    assert.equal(claimKeys[0], claimKeys[1]);
    await replacement.close();

    const byteBudget = await peer.evaluate(async () => {
      const queue = window.fixtureQueue;
      const { webLimits } = await import("/runtime.js");
      const original = webLimits.offline_max_bytes;
      try {
        webLimits.offline_max_bytes = window.fixtureRecord.imageBlob.size;
        await queue.enqueueUpload(window.fixtureRecord);
        let refused = false;
        try {
          await queue.enqueueUpload(window.fixtureRecord);
        } catch {
          refused = true;
        }
        const rows = (await queue.getSavedUploads()).items;
        await queue.discardSavedUpload(rows[0].id);
        return { refused, rows: rows.length };
      } finally {
        webLimits.offline_max_bytes = original;
      }
    });
    assert.deepEqual(byteBudget, { refused: true, rows: 1 });
    const admission = await peer.evaluate(async () => {
      const queue = window.fixtureQueue;
      const { webLimits } = await import("/runtime.js");
      for (let i = 0; i < webLimits.offline_max_entries; i++)
        await queue.enqueueUpload(window.fixtureRecord);
      let full = false;
      try {
        await queue.enqueueUpload(window.fixtureRecord);
      } catch {
        full = true;
      }
      const first = await queue.getSavedUploads();
      const second = await queue.getSavedUploads(first.next);
      const oldSession = await import("/session.js");
      const pending = queue
        .enqueueUpload({
          ...window.fixtureRecord,
          fileName: "must-not-change-account.png",
        })
        .catch(() => {});
      oldSession.replaceUserSession("account-b-token", {
        id: "b",
        email: "b@example.com",
        role: "USER",
        created_at: "2026-01-01",
      });
      await pending;
      const other = await queue.getSavedUploads();
      return {
        full,
        first: first.items.length,
        second: second.items.length,
        overlap: first.items.some((a) =>
          second.items.some((b) => a.id === b.id),
        ),
        other: other.items.length,
      };
    });
    assert.deepEqual(admission, {
      full: true,
      first: runtime.web.offline_page_size,
      second: runtime.web.offline_page_size,
      overlap: false,
      other: 0,
    });
    console.log(
      "PASS: account isolation, retained blobs, lost responses, cross-tab locking, cancellation, owner-tab closure, fallback fencing, failure states, cooldowns, and bounded pages",
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
