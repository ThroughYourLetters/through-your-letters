const assert = require("node:assert/strict");
const path = require("node:path");
const os = require("node:os");
const { chromium } = require("playwright");
const root = path.resolve(__dirname, "..");
const photo = (id, status = "PENDING") => ({
  id,
  status,
  image_url: "data:image/gif;base64,R0lGODlhAQABAAAAACw=",
  pin_code: "560001",
  contributor_tag: "Tester",
  detected_text: id,
  description: "Street lettering",
  created_at: "2026-11-01T00:00:00Z",
  likes_count: 0,
  comments_count: 0,
  report_count: status === "REPORTED" ? 3 : 0,
  report_reasons: ["Needs review"],
});
const comment = (id) => ({
  id,
  lettering_id: "photo",
  content: `Comment ${id}`,
  commenter_name: "Tester",
  commenter_email: "tester@example.test",
  status: "VISIBLE",
  moderation_score: 0,
  moderation_flags: [],
  auto_flagged: false,
  needs_review: true,
  review_priority: 1,
  created_at: "2026-11-01T00:00:00Z",
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
    await context.addInitScript(() => {
      sessionStorage.setItem("ttl_admin_token", "admin-a");
      localStorage.setItem(
        "ttl_admin_filter_presets_v1",
        JSON.stringify([
          {
            id: "broken",
            name: "Unsafe preset",
            type: "queue",
            payload: { queueLimit: 1e9 },
          },
        ]),
      );
    });
    await context.route("https://fonts.googleapis.com/**", (route) =>
      route.abort(),
    );
    await context.routeWebSocket("**/ws/feed", () => {});
    let pending = Array.from({ length: 27 }, (_, i) =>
      photo(`pending-${i + 1}`),
    );
    const comments = Array.from({ length: 27 }, (_, i) =>
      comment(`comment-${i + 1}`),
    );
    let failList = false;
    let expireList = false;
    let bulkRequests = 0;
    const queries = [];
    await context.route("**/api/v1/**", (route) => {
      const request = route.request();
      const url = new URL(request.url());
      const offset = Number(url.searchParams.get("offset") || 0);
      const limit = Number(url.searchParams.get("limit") || 25);
      if (url.pathname === "/api/v1/admin/stats")
        return route.fulfill({
          json: {
            total_uploads: 88,
            pending_approvals: pending.length,
            approved: 40,
            rejected: 21,
            total_cities: 7,
            total_likes: 16,
            total_comments: comments.length,
          },
        });
      if (url.pathname === "/api/v1/admin/moderation") {
        queries.push(Object.fromEntries(url.searchParams));
        if (expireList)
          return route.fulfill({
            status: 401,
            json: { error: "Authentication required" },
          });
        if (failList)
          return route.fulfill({
            status: 503,
            json: { error: "Review queue unavailable" },
          });
        const status = url.searchParams.get("status");
        const rows =
          status === "REPORTED"
            ? [photo("flagged-upload", "REPORTED")]
            : status === "APPROVED"
              ? [photo("approved-upload", "APPROVED")]
              : status === "REJECTED"
                ? []
                : pending;
        return route.fulfill({
          json: {
            items: rows.slice(offset, offset + limit),
            total: rows.length,
          },
        });
      }
      if (url.pathname === "/api/v1/admin/letterings/bulk") {
        bulkRequests++;
        const body = request.postDataJSON();
        assert.equal(body.action, "approve");
        assert.deepEqual(body.ids, ["pending-1", "pending-2"]);
        pending = pending.filter((item) => item.id !== "pending-1");
        return route.fulfill({
          json: {
            requested: 2,
            processed: 1,
            failed: 1,
            failed_items: [
              { id: "pending-2", error: "This upload changed during review" },
            ],
          },
        });
      }
      if (url.pathname === "/api/v1/admin/comments") {
        queries.push(Object.fromEntries(url.searchParams));
        let rows = comments;
        const status = url.searchParams.get("status");
        if (status && status !== "ALL")
          rows = rows.filter((item) => item.status === status);
        if (url.searchParams.has("q"))
          rows = rows.filter((item) =>
            item.content.includes(url.searchParams.get("q")),
          );
        if (url.searchParams.get("needs_review") === "true")
          rows = rows.filter((item) => item.needs_review);
        if (url.searchParams.has("min_score"))
          rows = rows.filter(
            (item) =>
              item.moderation_score >=
              Number(url.searchParams.get("min_score")),
          );
        return route.fulfill({
          json: {
            items: rows.slice(offset, offset + limit),
            total: rows.length,
            limit,
            offset,
          },
        });
      }
      if (url.pathname === "/api/v1/admin/comments/bulk") {
        const body = request.postDataJSON();
        assert.deepEqual(body.ids, ["comment-1", "comment-2"]);
        assert.equal(body.action, "hide");
        comments[0].status = "HIDDEN";
        return route.fulfill({
          json: {
            requested: 2,
            processed: 1,
            failed: 1,
            failed_items: [
              { id: "comment-2", error: "This comment changed during review" },
            ],
          },
        });
      }
      if (url.pathname === "/api/v1/cities") return route.fulfill({ json: [] });
      return route.fulfill({ json: { items: [], total: 0, letterings: [] } });
    });
    const page = await context.newPage();
    page.setDefaultTimeout(10_000);
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    let prompt = "Review reason";
    page.on("dialog", (dialog) =>
      dialog.type() === "prompt" ? dialog.accept(prompt) : dialog.accept(),
    );
    await page.goto(`${origin}/admin`);
    await page
      .getByRole("heading", { name: "pending-1", exact: true })
      .waitFor();
    assert.equal(
      await page.getByText("Unsafe preset", { exact: true }).count(),
      0,
    );
    await page
      .getByRole("checkbox", { name: "Select upload pending-1", exact: true })
      .check();
    await page
      .getByRole("checkbox", { name: "Select upload pending-2", exact: true })
      .check();
    await page
      .getByRole("button", { name: "Approve selected uploads", exact: true })
      .evaluate((button) => {
        button.click();
        button.click();
      });
    await page
      .getByText("1 upload updated; 1 failed", { exact: true })
      .waitFor();
    await page
      .getByRole("heading", { name: "pending-1", exact: true })
      .waitFor({ state: "detached" });
    assert.equal(bulkRequests, 1);
    assert.equal(
      await page
        .getByRole("checkbox", { name: "Select upload pending-2", exact: true })
        .isChecked(),
      true,
    );
    await page.getByRole("button", { name: "Next page", exact: true }).click();
    await page
      .getByRole("heading", { name: "pending-27", exact: true })
      .waitFor();
    assert.equal(
      await page.getByRole("checkbox", { checked: true }).count(),
      0,
    );
    await page
      .getByRole("button", { name: "Previous page", exact: true })
      .click();
    await page
      .getByRole("heading", { name: "pending-2", exact: true })
      .waitFor();
    await page.getByRole("button", { name: "APPROVED", exact: true }).click();
    await page
      .getByRole("heading", { name: "approved-upload", exact: true })
      .waitFor();
    await page
      .getByRole("combobox", { name: "Uploads per page" })
      .selectOption("10");
    prompt = "Approved reviews";
    await page
      .getByRole("button", { name: "Save filter preset", exact: true })
      .click();
    await page
      .getByRole("button", { name: "Apply Approved reviews", exact: true })
      .waitFor();
    await page.getByRole("button", { name: "PENDING", exact: true }).click();
    await page
      .getByRole("heading", { name: "pending-2", exact: true })
      .waitFor();
    await page
      .getByRole("button", { name: "Apply Approved reviews", exact: true })
      .click();
    await page
      .getByRole("heading", { name: "approved-upload", exact: true })
      .waitFor();
    assert.equal(
      await page
        .getByRole("combobox", { name: "Uploads per page" })
        .inputValue(),
      "10",
    );
    assert.equal(queries.at(-1).status, "APPROVED");
    assert.equal(queries.at(-1).offset, "0");
    await page.getByRole("button", { name: "Flags", exact: true }).focus();
    await page.keyboard.press("Enter");
    await page
      .getByRole("heading", { name: "flagged-upload", exact: true })
      .waitFor();
    assert.equal(
      await page
        .getByRole("checkbox", {
          name: "Select upload flagged-upload",
          exact: true,
        })
        .count(),
      1,
    );
    assert.equal(
      await page.getByRole("button", { name: "Approve", exact: true }).count(),
      0,
    );
    assert.equal(
      await page
        .getByRole("button", { name: "Clear selected flags", exact: true })
        .count(),
      1,
    );
    await page.getByText("3 Flags", { exact: true }).waitFor();
    await page.setViewportSize({ width: 320, height: 812 });
    const narrow = await page.evaluate(() => ({
      width: innerWidth,
      documentWidth: document.documentElement.scrollWidth,
      outside: [...document.querySelectorAll("body *")]
        .map((element) => ({
          element,
          bounds: element.getBoundingClientRect(),
        }))
        .filter(({ element, bounds }) => {
          if (bounds.right <= innerWidth + 2 || bounds.width < 1) return false;
          for (
            let parent = element.parentElement;
            parent;
            parent = parent.parentElement
          )
            if (
              ["auto", "hidden", "scroll"].includes(
                getComputedStyle(parent).overflowX,
              )
            )
              return false;
          return true;
        })
        .map(({ element, bounds }) => ({
          tag: element.tagName,
          class: element.className,
          right: bounds.right,
          text: element.textContent.slice(0, 80),
        }))
        .slice(0, 20),
    }));
    if (narrow.documentWidth > narrow.width + 2)
      await page.screenshot({
        path: path.join(os.tmpdir(), "ttl-admin-features-failure.png"),
        fullPage: true,
      });
    assert.ok(narrow.documentWidth <= narrow.width + 2, JSON.stringify(narrow));
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.getByRole("button", { name: "Comments", exact: true }).click();
    await page.getByText("Comment comment-1", { exact: true }).waitFor();
    await page.getByRole("button", { name: "Next page", exact: true }).click();
    await page.getByText("Comment comment-27", { exact: true }).waitFor();
    await page
      .getByRole("button", { name: "Previous page", exact: true })
      .click();
    await page
      .getByRole("checkbox", { name: "Select comment comment-1", exact: true })
      .check();
    await page
      .getByRole("checkbox", { name: "Select comment comment-2", exact: true })
      .check();
    await page
      .getByRole("button", { name: "Hide selected comments", exact: true })
      .click();
    await page
      .getByText("1 comment updated; 1 failed", { exact: true })
      .waitFor();
    await page.getByText(/This comment changed during review/).waitFor();
    assert.equal(
      await page
        .getByRole("checkbox", {
          name: "Select comment comment-2",
          exact: true,
        })
        .isChecked(),
      true,
    );
    await page.getByRole("button", { name: "HIDDEN", exact: true }).click();
    await page.getByText("Comment comment-1", { exact: true }).waitFor();
    assert.equal(
      await page.getByText("Comment comment-2", { exact: true }).count(),
      0,
    );
    await page
      .getByRole("textbox", { name: "Search comments" })
      .fill("comment-1");
    await page.getByRole("checkbox", { name: "Needs review only" }).check();
    await page.getByRole("spinbutton", { name: "Minimum score" }).fill("25");
    await page
      .getByRole("combobox", { name: "Sort comments" })
      .selectOption("newest");
    prompt = "Hidden review";
    await page
      .getByRole("button", { name: "Save filter preset", exact: true })
      .click();
    await page
      .getByRole("button", { name: "Apply Hidden review", exact: true })
      .waitFor();
    await page.getByRole("button", { name: "ALL", exact: true }).click();
    await page
      .getByRole("button", { name: "Apply Hidden review", exact: true })
      .click();
    assert.equal(
      await page
        .getByRole("button", { name: "HIDDEN", exact: true })
        .getAttribute("aria-pressed"),
      "true",
    );
    assert.equal(
      await page
        .getByRole("spinbutton", { name: "Minimum score" })
        .inputValue(),
      "25",
    );
    await page.getByRole("button", { name: "Activity", exact: true }).click();
    await page
      .getByRole("heading", { name: "Activity", exact: true })
      .waitFor();
    assert.equal(
      await page
        .locator("#admin-stats-title")
        .locator("..")
        .locator("dd")
        .first()
        .textContent(),
      "88",
    );
    await page.getByText("Akankshya Pradhan", { exact: true }).waitFor();
    await page.getByText("Vikalp Mishra", { exact: true }).waitFor();
    await page.setViewportSize({ width: 375, height: 812 });
    assert.ok(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth + 2,
      ),
    );
    await page.getByRole("button", { name: "Moderation", exact: true }).click();
    await page
      .getByRole("heading", { name: "pending-2", exact: true })
      .waitFor();
    failList = true;
    await page
      .getByRole("button", { name: "Refresh uploads", exact: true })
      .click();
    await page
      .getByText("Review queue unavailable", { exact: false })
      .waitFor();
    failList = false;
    await page.getByRole("button", { name: "Try again", exact: true }).click();
    await page
      .getByRole("heading", { name: "pending-2", exact: true })
      .waitFor();
    +(
      // A slow response to a previous filter must not replace the current list.
      (await page.evaluate(() => {
        const original = window.fetch;
        window.fetch = (input, init) => {
          const url = new URL(
            typeof input === "string" ? input : input.url || input.toString(),
            location.origin,
          );
          if (
            url.pathname === "/api/v1/admin/moderation" &&
            url.searchParams.get("status") === "PENDING"
          )
            return new Promise((resolve) => {
              window.releaseOldAdminList = () =>
                resolve(
                  new Response(
                    JSON.stringify({ items: [{ id: "late" }], total: 1 }),
                    { headers: { "Content-Type": "application/json" } },
                  ),
                );
            });
          return original(input, init);
        };
        window.restoreAdminFetch = () => {
          window.fetch = original;
        };
      }))
    );
    await page
      .getByRole("button", { name: "Refresh uploads", exact: true })
      .click();
    await page.waitForFunction(() => !!window.releaseOldAdminList);
    await page.getByRole("button", { name: "APPROVED", exact: true }).click();
    await page
      .getByRole("heading", { name: "approved-upload", exact: true })
      .waitFor();
    await page.evaluate(() => {
      window.releaseOldAdminList();
      window.restoreAdminFetch();
    });
    await page
      .getByRole("heading", { name: "approved-upload", exact: true })
      .waitFor();
    assert.equal(await page.getByText("late", { exact: true }).count(), 0);
    expireList = true;
    await page
      .getByRole("button", { name: "Refresh uploads", exact: true })
      .click();
    await page
      .getByRole("heading", { name: "Admin Portal", exact: true })
      .waitFor();
    assert.equal(
      await page.evaluate(() => sessionStorage.getItem("ttl_admin_token")),
      null,
    );
    await page.evaluate(() => {
      const original = window.fetch;
      window.fetch = (input, init) => {
        const url = new URL(
          typeof input === "string" ? input : input.url || input.toString(),
          location.origin,
        );
        if (url.pathname === "/api/v1/admin/login")
          return new Promise((resolve) => {
            window.releaseAdminLogin = async () => {
              resolve(
                new Response(JSON.stringify({ token: "late-admin" }), {
                  headers: { "Content-Type": "application/json" },
                }),
              );
              await new Promise((done) => setTimeout(done, 0));
              window.fetch = original;
            };
          });
        return original(input, init);
      };
    });
    await page
      .getByRole("textbox", { name: "Email", exact: true })
      .fill("admin@example.test");
    await page.getByLabel("Password", { exact: true }).fill("test-password");
    await page.getByRole("button", { name: "Sign in", exact: true }).click();
    await page.waitForFunction(() => !!window.releaseAdminLogin);
    await page
      .getByRole("navigation", { name: "Quick navigation" })
      .getByRole("link", { name: "Info", exact: true })
      .click();
    await page
      .getByRole("heading", { name: "Admin Portal", exact: true })
      .waitFor({ state: "detached" });
    await page.evaluate(() => window.releaseAdminLogin());
    assert.equal(
      await page.evaluate(() => sessionStorage.getItem("ttl_admin_token")),
      null,
    );
    assert.deepEqual(errors, []);
    console.log(
      "Admin features: filters, pagination, presets, partial failures, keyboard flags, real counts, narrow layout, recovery, stale reads, and expiry passed.",
    );
    await context.close();
  } finally {
    await browser.close();
    await server.close();
    process.chdir(previous);
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
