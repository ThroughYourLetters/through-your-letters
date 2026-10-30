const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium } = require("playwright");

(async () => {
  const previous = process.cwd();
  let server;
  let browser;
  try {
    let origin = process.env.TTL_PREVIEW_URL;
    if (!origin) {
      process.chdir(path.resolve(__dirname, "../apps/web"));
      const { preview } = await import("vite");
      server = await preview({
        preview: { host: "127.0.0.1", port: 0, strictPort: false },
        plugins: [
          {
            name: "pwa-test-api",
            configurePreviewServer(server) {
              server.middlewares.use((request, response, next) => {
                if (!request.url.startsWith("/api/")) return next();
                response.setHeader("content-type", "application/json");
                response.end(JSON.stringify({ email: "current-user-secret" }));
              });
            },
          },
        ],
      });
      origin = `http://127.0.0.1:${server.httpServer.address().port}`;
    }
    browser = await chromium.launch({ headless: true, channel: "chromium" });
    const context = await browser.newContext();
    await context.route("https://fonts.googleapis.com/**", (route) =>
      route.abort(),
    );
    const page = await context.newPage();
    page.setDefaultTimeout(20000);
    await page.goto(`${origin}/cache-policy.js`);
    await page.evaluate(async () => {
      const cache = await caches.open("api-cache");
      await cache.put(
        "/api/v1/auth/me",
        new Response(JSON.stringify({ email: "previous-user-secret" })),
      );
    });
    await page.goto(origin);
    await page.waitForFunction(
      () => navigator.serviceWorker.controller !== null,
    );
    assert.equal(
      await page.evaluate(() => caches.has("api-cache")),
      false,
      "Legacy private API cache was not deleted",
    );
    await page.evaluate(async () => {
      await fetch("/api/v1/auth/me").catch(() => {});
    });
    const apiEntries = await page.evaluate(async () => {
      const entries = [];
      for (const name of await caches.keys()) {
        for (const request of await (await caches.open(name)).keys()) {
          if (new URL(request.url).pathname.startsWith("/api/"))
            entries.push(request.url);
        }
      }
      return entries;
    });
    assert.deepEqual(apiEntries, [], "API response persisted in Cache Storage");
    await context.setOffline(true);
    const privateFetch = await page.evaluate(async () => {
      try {
        await fetch("/api/v1/auth/me");
        return "returned";
      } catch {
        return "network failure";
      }
    });
    assert.equal(
      privateFetch,
      "network failure",
      "Offline request returned old private data",
    );
    console.log(
      "Production service worker removes legacy private caches and never caches API responses",
    );
  } finally {
    await browser?.close();
    if (server)
      await new Promise((resolve, reject) =>
        server.httpServer.close((error) => (error ? reject(error) : resolve())),
      );
    process.chdir(previous);
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
