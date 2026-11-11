const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium } = require("playwright");
const root = path.resolve(__dirname, "..");
const image =
  "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
const user = {
  id: "reader",
  email: "reader@example.test",
  display_name: "Reader",
  role: "USER",
  created_at: "2026-01-01",
};
const upload = (n) => ({
  id: `upload-${n}`,
  image_url: image,
  thumbnail_small: image,
  pin_code: "560001",
  contributor_tag: "Reader",
  detected_text: `Upload ${n}`,
  description: null,
  status: "PENDING",
  likes_count: 0,
  comments_count: 0,
  report_count: 0,
  moderation_reason: null,
  moderated_at: null,
  moderated_by: null,
  created_at: "2026-11-01",
  updated_at: "2026-11-01",
});
const notification = (n) => ({
  id: `notice-${n}`,
  type: "review",
  title: `Notice ${n}`,
  body: "Review update",
  metadata: {},
  is_read: false,
  created_at: "2026-11-01",
});
const status = (n) => ({
  id: `status-${n}`,
  from_status: null,
  to_status: "PENDING",
  reason: `Status change ${n}`,
  actor_type: "SYSTEM",
  actor_sub: null,
  created_at: "2026-11-01",
});
const metadata = (n) => ({
  id: `metadata-${n}`,
  field_name: "description",
  old_value: null,
  new_value: `Metadata change ${n}`,
  created_at: "2026-11-01",
});

(async () => {
  const { createServer } = await import("vite");
  const previous = process.cwd();
  process.chdir(path.join(root, "apps/web"));
  const server = await createServer({
    mode: "test",
    server: { host: "127.0.0.1", port: 0 },
  });
  await server.listen();
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`;
  const browser = await chromium.launch({
    headless: true,
    channel: "chromium",
  });
  try {
    const context = await browser.newContext({ serviceWorkers: "block" });
    await context.addInitScript((user) => {
      sessionStorage.setItem("ttl_user_token", "reader-token");
      sessionStorage.setItem(
        "ttl_user_account",
        JSON.stringify({ token: "reader-token", user }),
      );
    }, user);
    await context.route("https://fonts.googleapis.com/**", (route) =>
      route.abort(),
    );
    await context.routeWebSocket("**/ws/feed", () => {});
    let failUploads = true;
    let failNotifications = true;
    let failTimeline = true;
    let failStatus = true;
    let failInitialNotifications = false;
    const uploadQueries = [];
    const notificationQueries = [];
    const timelineQueries = [];
    const readNotices = new Set();
    const error = (route) =>
      route.fulfill({
        status: 503,
        json: { error: "Temporarily unavailable" },
      });
    await context.route("**/api/v1/**", (route) => {
      const url = new URL(route.request().url());
      const after = url.searchParams.get("after");
      if (url.pathname === "/api/v1/cities") return route.fulfill({ json: [] });
      if (url.pathname === "/api/v1/me/letterings") {
        uploadQueries.push(Object.fromEntries(url.searchParams));
        if (after && failUploads) return error(route);
        const filtered = url.searchParams.get("status") === "APPROVED";
        return route.fulfill({
          json: {
            items: filtered
              ? []
              : after
                ? [upload(51), upload(52)]
                : Array.from({ length: 50 }, (_, i) => upload(i + 1)),
            total: filtered ? 0 : 52,
            limit: 50,
            offset: 0,
            next_cursor: filtered || after ? null : "uploads + next",
          },
        });
      }
      if (url.pathname.endsWith("/timeline")) {
        timelineQueries.push(Object.fromEntries(url.searchParams));
        if (failTimeline) return error(route);
        const section = url.searchParams.get("section");
        if (section === "status" && failStatus) {
          failStatus = false;
          return error(route);
        }
        return route.fulfill({
          json: {
            status_history:
              section === "metadata"
                ? []
                : after
                  ? [status(3)]
                  : [status(1), status(2)],
            metadata_history:
              section === "status" ? [] : after ? [metadata(2)] : [metadata(1)],
            next_status_cursor: section || after ? null : "status + next",
            next_metadata_cursor: section || after ? null : "metadata + next",
          },
        });
      }
      if (url.pathname === "/api/v1/me/notifications") {
        notificationQueries.push(Object.fromEntries(url.searchParams));
        if (failInitialNotifications || (after && failNotifications))
          return error(route);
        return route.fulfill({
          json: {
            items: (after
              ? [notification(101), notification(102)]
              : Array.from({ length: 100 }, (_, i) => notification(i + 1))
            ).map((item) => ({ ...item, is_read: readNotices.has(item.id) })),
            total: 102,
            unread: 102 - readNotices.size,
            limit: 100,
            offset: 0,
            next_cursor: after ? null : "notifications + next",
          },
        });
      }
      if (
        url.pathname.startsWith("/api/v1/me/notifications/") &&
        url.pathname.endsWith("/read")
      ) {
        readNotices.add(url.pathname.split("/").at(-2));
        return route.fulfill({ status: 200, body: "" });
      }
      if (url.pathname.endsWith("/auth/me"))
        return route.fulfill({ json: user });
      return route.fulfill({ json: { items: [], total: 0 } });
    });
    const page = await context.newPage();
    page.setDefaultTimeout(15_000);
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(`${origin}/me/uploads`);
    await page
      .getByText("Showing 50 of 52 uploads.", { exact: true })
      .waitFor();
    assert.equal(
      await page.getByRole("button", { name: "ALL (50)", exact: true }).count(),
      0,
    );
    await page
      .getByRole("button", { name: "Load more uploads", exact: true })
      .click();
    await page
      .getByText("Uploads could not be loaded.", { exact: false })
      .waitFor();
    assert.equal(await page.getByText("Upload 1", { exact: true }).count(), 1);
    failUploads = false;
    await page.getByRole("button", { name: "Try again", exact: true }).click();
    await page
      .getByText("Showing 52 of 52 uploads.", { exact: true })
      .waitFor();
    assert.ok(uploadQueries.some((query) => query.after === "uploads + next"));
    assert.equal(
      await page
        .getByRole("button", { name: "Load more uploads", exact: true })
        .count(),
      0,
    );

    await page
      .getByRole("button", { name: "Timeline", exact: true })
      .first()
      .click();
    await page
      .getByText("Timeline could not be loaded.", { exact: false })
      .waitFor();
    assert.equal(
      await page
        .getByText("No timeline entries available yet.", { exact: true })
        .count(),
      0,
    );
    failTimeline = false;
    await page.getByRole("button", { name: "Try again", exact: true }).click();
    await page.getByText("Status change 2", { exact: true }).waitFor();
    await page
      .getByRole("button", { name: "Load older status changes", exact: true })
      .click();
    await page
      .getByText("Older history could not be loaded. Please retry.", {
        exact: true,
      })
      .waitFor();
    assert.equal(
      await page.getByText("Status change 1", { exact: true }).count(),
      1,
    );
    await page
      .getByRole("button", { name: "Load older status changes", exact: true })
      .click();
    await page.getByText("Status change 3", { exact: true }).waitFor();
    await page
      .getByRole("button", { name: "Load older metadata changes", exact: true })
      .click();
    await page.getByText("Metadata change 2", { exact: false }).waitFor();
    assert.equal(
      await page.getByText("Status change 1", { exact: true }).count(),
      1,
    );
    assert.equal(
      await page.getByText("Metadata change 1", { exact: false }).count(),
      1,
    );
    assert.deepEqual(
      timelineQueries
        .filter((query) => query.section)
        .map((query) => [query.section, query.after]),
      [
        ["status", "status + next"],
        ["status", "status + next"],
        ["metadata", "metadata + next"],
      ],
    );
    await page.getByRole("button", { name: "APPROVED", exact: true }).click();
    await page
      .getByText("Showing 0 of 0 uploads with status APPROVED.", {
        exact: true,
      })
      .waitFor();
    assert.equal(await page.getByText("Upload 1", { exact: true }).count(), 0);

    await page.goto(`${origin}/me/notifications`);
    await page
      .getByRole("heading", { name: "Notice 100", exact: true })
      .waitFor();
    await page.getByText("Unread 102", { exact: true }).waitFor();
    await page
      .getByRole("button", { name: "Load more notifications", exact: true })
      .click();
    await page
      .getByText("Notifications could not be loaded.", { exact: false })
      .waitFor();
    assert.equal(
      await page
        .getByRole("heading", { name: "Notice 1", exact: true })
        .count(),
      1,
    );
    failNotifications = false;
    await page.getByRole("button", { name: "Try again", exact: true }).click();
    await page
      .getByRole("heading", { name: "Notice 102", exact: true })
      .waitFor();
    assert.ok(
      notificationQueries.some(
        (query) => query.after === "notifications + next",
      ),
    );
    assert.equal(
      await page
        .getByRole("button", { name: "Load more notifications", exact: true })
        .count(),
      0,
    );
    await page
      .getByRole("button", { name: "Mark Read", exact: true })
      .first()
      .click();
    await page.getByText("Unread 101", { exact: true }).waitFor();
    assert.equal(readNotices.size, 1);
    failInitialNotifications = true;
    await page.reload();
    await page
      .getByText("Notifications could not be loaded.", { exact: false })
      .waitFor();
    assert.equal(new URL(page.url()).pathname, "/me/notifications");
    assert.equal(
      await page.getByText("No notifications", { exact: true }).count(),
      0,
    );
    failInitialNotifications = false;
    await page.getByRole("button", { name: "Try again", exact: true }).click();
    await page
      .getByRole("heading", { name: "Notice 1", exact: true })
      .waitFor();
    assert.deepEqual(errors, []);
    await context.close();
    console.log(
      "Account pagination, independent histories, honest totals, and retained-page retries passed",
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
