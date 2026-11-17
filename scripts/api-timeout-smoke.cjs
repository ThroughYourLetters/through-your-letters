const assert = require("node:assert/strict");
const http = require("node:http");
const path = require("node:path");
const { chromium } = require("playwright");

(async () => {
  const { createServer } = await import("vite");
  const { publicRuntimeLimits } =
    await import("../apps/web/src/lib/publicRuntimeLimits.ts");
  const budgets = publicRuntimeLimits(
    require("../apps/api/config/runtime.json"),
  );
  const closed = new Set();
  const responses = new Set();
  const backend = http.createServer((request, response) => {
    response.setHeader("access-control-allow-origin", "*");
    response.setHeader(
      "access-control-allow-headers",
      "authorization, content-type",
    );
    if (request.method === "OPTIONS") {
      response.writeHead(204);
      response.end();
      return;
    }
    responses.add(response);
    response.once("close", () => {
      closed.add(request.url);
      responses.delete(response);
    });
    if (request.url.startsWith("/api/v1/letterings")) {
      response.writeHead(200, { "content-type": "application/json" });
      response.write('{"letterings":[');
    } else if (request.url.startsWith("/api/v1/me/notifications")) {
      response.writeHead(401, { "content-type": "application/json" });
      response.write('{"error":');
    }
    // The cities request never receives headers; other requests never finish their bodies.
  });
  await new Promise((resolve) => backend.listen(0, "127.0.0.1", resolve));
  const previous = process.cwd();
  process.chdir(path.resolve(__dirname, "../apps/web"));
  const server = await createServer({
    mode: "test",
    optimizeDeps: { noDiscovery: true },
    define: {
      "import.meta.env.VITE_API_URL": JSON.stringify(
        `http://127.0.0.1:${backend.address().port}`,
      ),
      __TTL_RUNTIME_LIMITS__: JSON.stringify({
        ...budgets,
        web: { ...budgets.web, request_timeout_ms: 1000 },
      }),
    },
    plugins: [
      {
        name: "timeout-fixture",
        configureServer(server) {
          server.middlewares.use(
            "/timeout-fixture",
            async (_request, response) => {
              response.setHeader("content-type", "text/html");
              response.end(
                await server.transformIndexHtml(
                  "/timeout-fixture",
                  "<!doctype html><title>Request deadline fixture</title>",
                ),
              );
            },
          );
        },
      },
    ],
    server: {
      host: "127.0.0.1",
      port: 0,
      strictPort: false,
      proxy: {
        "/api": { target: `http://127.0.0.1:${backend.address().port}` },
      },
    },
  });
  let browser;
  try {
    await server.listen();
    browser = await chromium.launch({ headless: true, channel: "chromium" });
    const page = await browser.newPage();
    await page.goto(
      `http://127.0.0.1:${server.httpServer.address().port}/timeout-fixture`,
    );
    const results = await page.evaluate(async () => {
      const { api } = await import("/src/lib/api.ts");
      const session = await import("/src/lib/userSession.ts");
      session.replaceUserSession("deadline-token", {
        id: "deadline-user",
        email: "deadline@example.test",
        role: "USER",
      });
      const results = [];
      for (const action of [
        () => api.getCities(),
        () => api.getGallery(),
        () => api.getNotifications(),
      ]) {
        const started = performance.now();
        try {
          await action();
          results.push({ name: "unexpected success" });
        } catch (error) {
          results.push({
            name: error.name,
            message: error.message,
            elapsed: performance.now() - started,
          });
        }
      }
      return { results, token: session.captureUserSession().token };
    });
    for (const result of results.results) {
      assert.equal(result.name, "TimeoutError");
      assert.equal(result.message, "Request timed out");
      assert.ok(result.elapsed < 3000, JSON.stringify(result));
    }
    assert.equal(
      results.token,
      "deadline-token",
      "A timed-out 401 must not expire the session",
    );
    const closeDeadline = Date.now() + 2000;
    while (closed.size < 3 && Date.now() < closeDeadline)
      await new Promise((resolve) => setTimeout(resolve, 10));
    assert.equal(
      closed.size,
      3,
      "Timed-out requests must close their backend connections",
    );
    console.log(
      "Header, body, and 401 deadlines abort real HTTP requests without expiring the session",
    );
  } finally {
    await browser?.close();
    await server.close();
    for (const response of responses) response.destroy();
    backend.closeAllConnections();
    await new Promise((resolve) => backend.close(resolve));
    process.chdir(previous);
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
