const assert = require("node:assert/strict");
const fs = require("node:fs/promises");
const http = require("node:http");
const path = require("node:path");
const os = require("node:os");
const { chromium } = require("playwright");

(async () => {
  const { build } = await import("vite");
  const previous = process.cwd();
  const root = path.resolve(__dirname, "../apps/web");
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "ttl-deployment-"));
  let browser, server;
  process.chdir(root);
  try {
    for (const revision of ["old", "new"]) {
      await build({
        root,
        mode: "test",
        logLevel: "warn",
        define: {
          "import.meta.env.VITE_API_URL": JSON.stringify("same-origin"),
        },
        build: { outDir: path.join(directory, revision), emptyOutDir: true },
        plugins: [
          {
            name: "deployment-fixture",
            enforce: "pre",
            transform(code, id) {
              if (id.endsWith("/MapSection.tsx"))
                return {
                  code: `${code}\ndocument.documentElement.dataset.deployment = ${JSON.stringify(revision)};`,
                  map: null,
                };
            },
          },
        ],
      });
    }
    let active = "old";
    const missing = [];
    server = http.createServer(async (request, response) => {
      const pathname = new URL(request.url, "http://fixture.test").pathname;
      response.setHeader("cache-control", "no-store");
      if (pathname.startsWith("/api/")) {
        let body = [];
        if (pathname === "/api/v1/letterings")
          body = { letterings: [], total: 0, limit: 30, offset: 0 };
        if (pathname === "/api/v1/geo/density")
          body = { cells: [], truncated: false };
        if (pathname === "/api/v1/geo/summary") body = { unmapped_count: 0 };
        if (pathname === "/api/v1/geo/photos")
          body = { items: [], next_cursor: null };
        response.setHeader("content-type", "application/json");
        response.end(JSON.stringify(body));
        return;
      }
      const relative =
        pathname === "/" || !path.extname(pathname)
          ? "index.html"
          : pathname.slice(1);
      const target = path.resolve(directory, active, relative);
      if (!target.startsWith(path.join(directory, active) + path.sep)) {
        response.writeHead(400);
        response.end();
        return;
      }
      try {
        const body = await fs.readFile(target);
        response.setHeader(
          "content-type",
          {
            ".html": "text/html",
            ".js": "application/javascript",
            ".css": "text/css",
            ".svg": "image/svg+xml",
            ".webmanifest": "application/manifest+json",
          }[path.extname(target)] || "application/octet-stream",
        );
        response.end(body);
      } catch {
        missing.push(pathname);
        response.writeHead(404);
        response.end("Asset not found");
      }
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const origin = `http://127.0.0.1:${server.address().port}`;
    browser = await chromium.launch({ headless: true, channel: "chromium" });
    const setup = async (context) => {
      await context.routeWebSocket("**/ws/feed", () => {});
      await context.route("https://fonts.googleapis.com/**", (route) =>
        route.abort(),
      );
      await context.route("https://*.tile.openstreetmap.org/**", (route) =>
        route.abort(),
      );
    };
    const plain = await browser.newContext({ serviceWorkers: "block" });
    await setup(plain);
    const page = await plain.newPage();
    page.setDefaultTimeout(15000);
    await page.goto(origin);
    await page
      .getByRole("link", { name: "Map", exact: true })
      .first()
      .waitFor();
    active = "new";
    await page.getByRole("link", { name: "Map", exact: true }).first().click();
    await page
      .getByRole("heading", { name: "This page could not load" })
      .waitFor();
    assert.ok(
      missing.some((url) => /MapSection-.*\.js$/.test(url)),
      "The old lazy chunk must actually return 404",
    );
    assert.equal(
      await page
        .getByRole("alert")
        .getByText("Failed to fetch", { exact: false })
        .count(),
      0,
    );
    await page
      .getByRole("button", { name: "Reload page", exact: true })
      .click();
    await page.getByRole("heading", { name: "Map", exact: true }).waitFor();
    assert.equal(
      await page.evaluate(() => document.documentElement.dataset.deployment),
      "new",
    );
    await plain.close();
    console.log(
      "An old tab recovers from a real missing lazy chunk by loading the new build",
    );

    active = "old";
    const installed = await browser.newContext();
    await setup(installed);
    const first = await installed.newPage();
    first.setDefaultTimeout(20000);
    await first.goto(origin);
    await first.waitForFunction(() => !!navigator.serviceWorker.controller);
    const second = await installed.newPage();
    second.setDefaultTimeout(20000);
    await second.goto(`${origin}/auth`);
    await second
      .getByLabel("Email", { exact: true })
      .fill("unfinished@example.test");
    active = "new";
    await first.evaluate(async () =>
      (await navigator.serviceWorker.getRegistration()).update(),
    );
    await first
      .getByRole("status")
      .filter({ hasText: "An update is ready" })
      .waitFor();
    await second
      .getByRole("status")
      .filter({ hasText: "An update is ready" })
      .waitFor();
    const missingBefore = missing.length;
    await first.getByRole("link", { name: "Map", exact: true }).first().click();
    await first.getByRole("heading", { name: "Map", exact: true }).waitFor();
    assert.equal(
      await first.evaluate(() => document.documentElement.dataset.deployment),
      "old",
    );
    assert.equal(
      missing.length,
      missingBefore,
      "The waiting update must retain the old tab's lazy chunks",
    );
    await first.getByRole("button", { name: "Reload", exact: true }).click();
    await first.waitForFunction(
      () => document.documentElement.dataset.deployment === "new",
    );
    assert.equal(
      await second.getByLabel("Email", { exact: true }).inputValue(),
      "unfinished@example.test",
      "Updating one tab must not reload another tab's unsaved form",
    );
    await second.getByRole("button", { name: "Reload", exact: true }).click();
    await second.getByLabel("Email", { exact: true }).waitFor();
    assert.equal(
      await second.getByLabel("Email", { exact: true }).inputValue(),
      "",
    );
    await installed.close();
    console.log(
      "Service-worker updates preserve old chunks and other tabs until each user chooses to reload",
    );
  } finally {
    await browser?.close();
    server?.closeAllConnections();
    if (server) await new Promise((resolve) => server.close(resolve));
    await fs.rm(directory, { recursive: true, force: true });
    process.chdir(previous);
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
