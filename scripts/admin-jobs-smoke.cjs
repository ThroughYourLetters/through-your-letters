// Exercise the real admin UI with isolated HTTP responses.
const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium } = require("playwright");
const root = path.resolve(__dirname, "..");
const finished = "2026-10-30T12:00:00Z";
const analysis = (id, canRetry = true, finishedAt = finished) => ({
  id,
  contributor_tag: `Contributor ${id}`,
  thumbnail_small: "",
  attempts: 3,
  max_attempts: 3,
  last_error_code: "interrupted",
  finished_at: finishedAt,
  can_retry: canRetry,
});

(async () => {
  const { createServer } = await import("vite");
  const previous = process.cwd();
  process.chdir(path.join(root, "apps/web"));
  const server = await createServer({
    mode: "test",
    server: { host: "127.0.0.1", port: 0, strictPort: false },
  });
  await server.listen();
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`;
  const browser = await chromium.launch({
    headless: true,
    channel: "chromium",
  });
  try {
    const context = await browser.newContext({ serviceWorkers: "block" });
    await context.addInitScript(() =>
      sessionStorage.setItem("ttl_admin_token", "admin-a"),
    );
    await context.route("https://fonts.googleapis.com/**", (route) =>
      route.abort(),
    );
    await context.routeWebSocket("**/ws/feed", () => {});
    let failList = false;
    let expireList = false;
    let retried = false;
    let cleanupRetried = false;
    let conflictOnce = true;
    let analysisFinished = finished;
    const retries = [];
    const reads = [];
    await context.route("**/api/v1/**", async (route) => {
      const request = route.request();
      const url = new URL(request.url());
      if (url.pathname === "/api/v1/admin/stats") {
        return route.fulfill({
          json: {
            total_uploads: 0,
            pending_approvals: 0,
            approved: 0,
            rejected: 0,
            total_cities: 0,
            total_likes: 0,
            total_comments: 0,
          },
        });
      }
      if (url.pathname === "/api/v1/admin/jobs/analysis") {
        reads.push(url.searchParams.get("after"));
        if (expireList)
          return route.fulfill({
            status: 401,
            json: { error: "Authentication required" },
          });
        if (failList)
          return route.fulfill({
            status: 503,
            json: { error: "Jobs are temporarily unavailable" },
          });
        const items = url.searchParams.has("after")
          ? [analysis("reviewed", false)]
          : retried
            ? []
            : [analysis("pending", true, analysisFinished)];
        return route.fulfill({
          json: {
            items,
            next_cursor:
              url.searchParams.has("after") || retried ? null : "next-analysis",
          },
        });
      }
      if (url.pathname === "/api/v1/admin/jobs/cleanup") {
        return route.fulfill({
          json: {
            items: cleanupRetried
              ? []
              : [
                  {
                    id: "cleanup",
                    state: "ready",
                    attempts: 7,
                    available_at: finished,
                    last_error_code: "storage_unavailable",
                  },
                ],
            next_cursor: null,
          },
        });
      }
      if (url.pathname.endsWith("/retry")) {
        retries.push({
          path: url.pathname,
          body: request.postDataJSON(),
          authorization: request.headers().authorization,
        });
        if (url.pathname.includes("/analysis/") && conflictOnce) {
          conflictOnce = false;
          analysisFinished = "2026-10-30T12:01:00Z";
          return route.fulfill({
            status: 409,
            json: { error: "The job changed; refresh before retrying" },
          });
        }
        if (url.pathname.includes("/analysis/")) retried = true;
        else cleanupRetried = true;
        return route.fulfill({ status: 204 });
      }
      if (url.pathname === "/api/v1/cities") return route.fulfill({ json: [] });
      if (url.pathname === "/api/v1/letterings")
        return route.fulfill({ json: { letterings: [], total: 0 } });
      return route.fulfill({
        json: { items: [], total: 0, letterings: [], cities: [] },
      });
    });
    const page = await context.newPage();
    page.on("pageerror", (error) =>
      console.error("Browser error:", error.message),
    );
    await page.goto(`${origin}/admin`);
    await page
      .getByRole("button", { name: "Failed jobs", exact: true })
      .click();
    await page.getByText("Contributor pending", { exact: true }).waitFor();
    await page.getByRole("button", { name: "Next page", exact: true }).click();
    await page.getByText("Contributor reviewed", { exact: true }).waitFor();
    assert.equal(
      await page
        .getByRole("button", { name: "Retry analysis", exact: true })
        .isDisabled(),
      true,
    );
    await page.getByRole("button", { name: "First page", exact: true }).click();
    await page.getByText("Contributor pending", { exact: true }).waitFor();
    failList = true;
    await page
      .getByRole("button", { name: "Refresh jobs", exact: true })
      .click();
    await page
      .getByRole("alert")
      .filter({ hasText: "Jobs are temporarily unavailable" })
      .waitFor();
    failList = false;
    await page
      .getByRole("button", { name: "Refresh jobs", exact: true })
      .click();
    await page.getByText("Contributor pending", { exact: true }).waitFor();
    await page
      .getByRole("button", { name: "Retry analysis", exact: true })
      .click();
    await page
      .getByRole("alert")
      .filter({ hasText: "The job changed; refresh before retrying" })
      .waitFor();
    assert.equal(
      await page.getByText("Retry queued.", { exact: true }).count(),
      0,
    );
    await page
      .getByRole("button", { name: "Refresh jobs", exact: true })
      .click();
    await page.getByText("Contributor pending", { exact: true }).waitFor();
    await page
      .getByRole("button", { name: "Retry analysis", exact: true })
      .click();
    await page.getByText("Retry queued.", { exact: true }).waitFor();
    await page
      .getByText("No failed jobs on this page.", { exact: true })
      .waitFor();
    await page.getByLabel("Job type").selectOption("cleanup");
    await page.getByText("7 deletion attempts", { exact: true }).waitFor();
    await page
      .getByRole("button", { name: "Retry cleanup now", exact: true })
      .click();
    await page
      .getByText("No failed jobs on this page.", { exact: true })
      .waitFor();
    assert.deepEqual(retries, [
      {
        path: "/api/v1/admin/jobs/analysis/pending/retry",
        body: { finished_at: finished },
        authorization: "Bearer admin-a",
      },
      {
        path: "/api/v1/admin/jobs/analysis/pending/retry",
        body: { finished_at: "2026-10-30T12:01:00Z" },
        authorization: "Bearer admin-a",
      },
      {
        path: "/api/v1/admin/jobs/cleanup/cleanup/retry",
        body: { available_at: finished, attempts: 7 },
        authorization: "Bearer admin-a",
      },
    ]);
    assert(reads.includes("next-analysis"));
    expireList = true;
    await page.getByLabel("Job type").selectOption("analysis");
    await page.getByPlaceholder("Password", { exact: true }).waitFor();
    assert.equal(
      await page.evaluate(() => sessionStorage.getItem("ttl_admin_token")),
      null,
    );

    // Old responses may neither disclose job data nor clear a replacement credential.
    for (const status of [200, 401]) {
      let release;
      let started;
      const waiting = new Promise((resolve) => {
        started = resolve;
      });
      const blocked = new Promise((resolve) => {
        release = resolve;
      });
      await page.route("**/api/v1/admin/jobs/analysis", async (route) => {
        started();
        await blocked;
        await route.fulfill({
          status,
          json:
            status === 200
              ? { items: [analysis("private-old-job")], next_cursor: null }
              : { error: "Authentication required" },
        });
      });
      await page.evaluate(async () => {
        const { api } = await import("/src/lib/api.ts");
        sessionStorage.setItem("ttl_admin_token", "old-admin");
        window.adminRequest = api.adminAnalysisJobs().then(
          () => "disclosed",
          (error) => error.name,
        );
      });
      await waiting;
      await page.evaluate(() =>
        sessionStorage.setItem("ttl_admin_token", "new-admin"),
      );
      release();
      assert.equal(
        await page.evaluate(() => window.adminRequest),
        "AbortError",
      );
      assert.equal(
        await page.evaluate(() => sessionStorage.getItem("ttl_admin_token")),
        "new-admin",
      );
      await page.unroute("**/api/v1/admin/jobs/analysis");
    }
    await context.close();
    console.log(
      "Admin jobs: pagination, safe retries, error recovery, session expiry, and stale response isolation passed",
    );
  } finally {
    await browser.close();
    await server.close();
    process.chdir(previous);
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
