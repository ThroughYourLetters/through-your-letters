const assert = require("node:assert/strict");
const { chromium } = require("playwright");
const origin = process.env.TTL_PREVIEW_URL || "http://localhost:5173";
const image =
  "data:image/svg+xml," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="100" height="80"><rect width="100" height="80" fill="#cc543a"/></svg>',
  );
const record = (id) => ({
  id,
  city_name: "Bengaluru",
  country_code: "IN",
  pin_code: "560025",
  image_url: image,
  lat: 12.9716,
  lng: 77.5946,
  location_source: "legacy_unverified",
  location_error: null,
  updated_at: "2026-10-28T00:00:00Z",
});
(async () => {
  const browser = await chromium.launch({
    headless: true,
    channel: "chromium",
  });
  try {
    for (const width of [1440, 390, 320]) {
      const context = await browser.newContext({
        viewport: { width, height: 900 },
        serviceWorkers: "block",
        reducedMotion: "reduce",
      });
      await context.addInitScript(() =>
        sessionStorage.setItem("ttl_admin_token", "fixture"),
      );
      await context.routeWebSocket("**/ws/feed", () => {});
      await context.route("https://fonts.googleapis.com/**", (r) => r.abort());
      let failReport = true,
        empty = false,
        applied = null,
        previews = 0,
        applyAttempts = [];
      await context.route("**/api/v1/**", async (route) => {
        const req = route.request(),
          url = new URL(req.url()),
          path = url.pathname;
        let data = {};
        if (path.endsWith("/location-repairs")) {
          if (failReport)
            return route.fulfill({
              status: 503,
              json: { error: "Unavailable" },
            });
          data = {
            items:
              empty || (applied && !url.searchParams.has("include_resolved"))
                ? []
                : [record(url.searchParams.get("after") ? "second" : "first")],
            next_cursor:
              empty || applied || url.searchParams.has("after")
                ? null
                : "first",
          };
        } else if (path.endsWith("/preview")) {
          const evidence = req.postDataJSON();
          if (evidence.country_code !== "IN")
            return route.fulfill({
              status: 400,
              json: { error: "The source country must match the upload" },
            });
          data = {
            repair_id: `repair-${++previews}`,
            current: record("first"),
            evidence,
          };
        } else if (path.endsWith("/apply")) {
          const body = req.postDataJSON();
          applyAttempts.push(body);
          applied = applied || {
            id: body.repair_id,
            lettering_id: "first",
            reviewed_by: "Reviewer",
            before_location: record("first"),
            after_location: {
              ...record("first"),
              lat: body.evidence.lat,
              lng: body.evidence.lng,
              location_source: "postal_code",
            },
            evidence: body.evidence,
            created_at: "2026-10-28T01:00:00Z",
          };
          if (applyAttempts.length === 1) return route.abort("failed");
          data = applied;
        } else if (path.endsWith("/history"))
          data = { items: applied ? [applied] : [], next_cursor: null };
        else if (path.endsWith("/moderation")) data = { items: [], total: 0 };
        else if (path.endsWith("/cities")) data = [];
        else if (path.endsWith("/stats"))
          data = {
            total_uploads: 2,
            pending_approvals: 0,
            approved: 2,
            rejected: 0,
            total_cities: 1,
            total_likes: 0,
            total_comments: 0,
          };
        else if (path.includes("/notifications"))
          data = { items: [], total: 0, unread_count: 0 };
        await route.fulfill({ status: 200, json: data });
      });
      const page = await context.newPage(),
        errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto(`${origin}/admin`);
      await page
        .getByRole("button", { name: "Locations", exact: true })
        .click();
      await page
        .getByText("Could not load location report.", { exact: false })
        .waitFor();
      failReport = false;
      await page.getByRole("button", { name: "Retry report" }).click();
      await page.getByRole("button", { name: "Review location" }).waitFor();
      await page.getByRole("button", { name: "Next page" }).click();
      await page
        .getByRole("button", { name: "Next page" })
        .isDisabled()
        .then(assert.ok);
      await page.getByRole("button", { name: "Previous page" }).click();
      await page.getByRole("button", { name: "Review location" }).click();
      const fields = {
        "Source city": "Bengaluru",
        "Source country code": "GB",
        "Source postal code": "560025",
        Latitude: "12.95",
        Longitude: "77.61",
        "Source name": "Postal authority fixture",
        "Source URL": "https://example.test/postal/560025",
        "Reason for correction": "Checked against the postal source",
      };
      for (const [label, value] of Object.entries(fields))
        await page.getByLabel(label, { exact: true }).fill(value);
      await page
        .getByRole("button", { name: "Preview correction", exact: true })
        .click();
      await page
        .getByText("The source country must match the upload", { exact: true })
        .waitFor();
      assert.equal(applyAttempts.length, 0);
      await page.getByLabel("Source country code", { exact: true }).fill("IN");
      await page
        .getByRole("button", { name: "Preview correction", exact: true })
        .click();
      const preview = page.getByRole("region", { name: "Correction preview" });
      await preview.waitFor();
      assert.ok(
        await page
          .getByRole("button", { name: "Approve correction" })
          .isDisabled(),
      );
      await page.getByLabel("Latitude", { exact: true }).fill("12.951");
      assert.equal(await preview.count(), 0);
      await page
        .getByRole("button", { name: "Preview correction", exact: true })
        .click();
      await preview.waitFor();
      await page
        .getByRole("checkbox", { name: /I checked the coordinates/ })
        .check();
      await page.getByRole("button", { name: "Approve correction" }).click();
      await page
        .getByRole("alert")
        .filter({ hasText: "Preview again" })
        .waitFor();
      await page.getByRole("button", { name: "Approve correction" }).click();
      await page
        .getByText("Correction saved for Bengaluru 560025.", { exact: false })
        .waitFor();
      assert.equal(applyAttempts.length, 2);
      assert.deepEqual(applyAttempts[0], applyAttempts[1]);
      await page
        .getByRole("button", { name: "View correction history" })
        .click();
      await page
        .getByRole("region", { name: "Correction history" })
        .getByText("Reviewer", { exact: false })
        .waitFor();
      await page.getByRole("button", { name: "Back to report" }).click();
      await page
        .getByText("No uploads need review on this page.", { exact: true })
        .waitFor();
      await page
        .getByRole("checkbox", { name: "Show resolved uploads" })
        .check();
      await page.getByRole("button", { name: "Review location" }).click();
      await page
        .getByRole("region", { name: "Correction history" })
        .getByText("Reviewer", { exact: false })
        .waitFor();
      assert.equal(
        await page.evaluate(
          () => document.documentElement.scrollWidth > innerWidth,
        ),
        false,
      );
      assert.deepEqual(errors, []);
      console.log(
        `PASS ${width}px: report retry, pagination, source errors, preview invalidation, review confirmation, lost-response retry, history and empty report`,
      );
      await context.close();
    }
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
