const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium } = require("playwright");

const root = path.resolve(__dirname, "..");
const image =
  "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
const id = (n) => `00000000-0000-0000-0000-${String(n).padStart(12, "0")}`;
const lettering = (n) => ({
  id: id(n),
  city_id: id(1000),
  contributor_tag: "Walker",
  image_url: image,
  thumbnail_urls: { small: image, medium: image, large: image },
  location: null,
  location_source: "unresolved",
  pin_code: "560001",
  detected_text: `Sign ${n}`,
  ml_metadata: null,
  description: "Street lettering",
  is_lettering: true,
  status: "APPROVED",
  likes_count: 0,
  comments_count: 52,
  cultural_context: null,
  created_at: "2026-11-01T00:00:00Z",
  updated_at: "2026-11-01T00:00:00Z",
  is_owner: false,
});
const comment = (n) => ({
  id: id(2000 + n),
  lettering_id: id(1),
  content: `Note ${n}`,
  commenter_name: "Reader",
  status: "VISIBLE",
  created_at: "2026-11-01T00:00:00Z",
  updated_at: "2026-11-01T00:00:00Z",
});
const challenge = (n) => ({
  id: id(3000 + n),
  title: `Challenge ${n}`,
  target_count: 100,
  current_count: n,
  status: "ACTIVE",
});
const revisit = (n) => ({
  id: id(4000 + n),
  original_lettering_id: id(1),
  revisit_lettering_id: id(n + 1),
  notes: `Capture ${n}`,
  created_at: "2026-11-01T00:00:00Z",
  original: { image_url: image, created_at: "2026-10-01T00:00:00Z" },
  revisit: { image_url: image, created_at: "2026-11-01T00:00:00Z" },
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
    await context.route("https://fonts.googleapis.com/**", (route) =>
      route.abort(),
    );
    await context.routeWebSocket("**/ws/feed", () => {});
    let legacyGallery = false;
    let failComments = true;
    let failOlderComments = true;
    let failChallenges = true;
    let failRevisits = false;
    const galleryRequests = [];
    const commentsRequests = [];
    const challengesRequests = [];
    const revisitRequests = [];
    const cursorHeaders = (url, token) => ({
      Link: `<${url.pathname}?limit=50&after=${encodeURIComponent(token)}>; rel="next"`,
      "Access-Control-Expose-Headers": "Link",
    });
    await context.route("**/api/v1/**", (route) => {
      const url = new URL(route.request().url());
      const after = url.searchParams.get("after");
      const error = () =>
        route.fulfill({
          status: 503,
          json: { error: "Temporarily unavailable" },
        });
      if (url.pathname === "/api/v1/letterings") {
        galleryRequests.push(Object.fromEntries(url.searchParams));
        const continued =
          after !== null || Number(url.searchParams.get("offset")) > 0;
        const body = {
          letterings: continued
            ? [lettering(31), lettering(32)]
            : Array.from({ length: 30 }, (_, i) => lettering(i + 1)),
          total: 32,
          limit: 30,
          offset: continued ? 30 : 0,
        };
        if (!legacyGallery)
          body.next_cursor = continued ? null : "gallery + next";
        return route.fulfill({ json: body });
      }
      if (url.pathname === `/api/v1/letterings/${id(1)}`)
        return route.fulfill({ json: lettering(1) });
      if (url.pathname.endsWith("/comments")) {
        commentsRequests.push(after);
        if (!after && failComments) {
          return error();
        }
        if (after && failOlderComments) {
          failOlderComments = false;
          return error();
        }
        return route.fulfill({
          headers: after ? {} : cursorHeaders(url, "comments + next"),
          json: after
            ? [comment(51), comment(52)]
            : Array.from({ length: 50 }, (_, i) => comment(i + 1)),
        });
      }
      if (url.pathname.endsWith("/revisits")) {
        revisitRequests.push(after);
        if (failRevisits) {
          return error();
        }
        return route.fulfill({
          json: {
            revisits: [revisit(after ? 2 : 1)],
            next_cursor: after ? null : "revisits + next",
          },
        });
      }
      if (url.pathname.endsWith("/similar"))
        return route.fulfill({ json: { similar: [] } });
      if (url.pathname === "/api/v1/challenges") {
        challengesRequests.push(after);
        if (after && failChallenges) {
          failChallenges = false;
          return error();
        }
        return route.fulfill({
          headers: after ? {} : cursorHeaders(url, "challenges + next"),
          json: after
            ? [challenge(51), challenge(52)]
            : Array.from({ length: 50 }, (_, i) => challenge(i + 1)),
        });
      }
      if (
        url.pathname === "/api/v1/cities" ||
        url.pathname === "/api/v1/community/leaderboard"
      )
        return route.fulfill({ json: [] });
      return route.fulfill({ json: { items: [], total: 0 } });
    });
    const page = await context.newPage();
    page.setDefaultTimeout(15_000);
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));

    for (const legacy of [false, true]) {
      legacyGallery = legacy;
      galleryRequests.length = 0;
      await page.goto(origin);
      await page
        .getByRole("button", { name: "Open Sign 30", exact: true })
        .scrollIntoViewIfNeeded();
      await page
        .locator(".masonry")
        .evaluate((grid) =>
          grid.nextElementSibling.scrollIntoView({ block: "center" }),
        );
      await page
        .getByRole("button", { name: "Open Sign 32", exact: true })
        .waitFor();
      const next = galleryRequests.find(
        (query) => query.after || Number(query.offset) > 0,
      );
      assert.ok(next, "Gallery requests a second page");
      if (legacy) assert.equal(next.offset, "30");
      else assert.equal(next.after, "gallery + next");
      assert.equal(
        await page
          .getByRole("button", { name: "Open Sign 1", exact: true })
          .count(),
        1,
      );
      await page.getByText("You've reached the end of the archive").waitFor();
    }

    await page.goto(`${origin}/lettering/${id(1)}`);
    await page
      .getByRole("heading", { name: "Notes & Comments (52)" })
      .waitFor();
    await page
      .getByText("Comments could not be loaded.", { exact: false })
      .waitFor();
    assert.equal(
      await page.getByText("No notes yet.", { exact: false }).count(),
      0,
    );
    failComments = false;
    await page.getByRole("button", { name: "Try again", exact: true }).click();
    await page.getByText("Note 50", { exact: true }).waitFor();
    await page
      .getByRole("button", { name: "Load older comments", exact: true })
      .click();
    await page
      .getByText("Older comments could not be loaded. Please retry.", {
        exact: true,
      })
      .waitFor();
    assert.equal(await page.getByText("Note 1", { exact: true }).count(), 1);
    await page
      .getByRole("button", { name: "Load older comments", exact: true })
      .click();
    await page.getByText("Note 52", { exact: true }).waitFor();
    assert.equal(
      await page
        .getByRole("button", { name: "Load older comments", exact: true })
        .count(),
      0,
    );
    assert.ok(commentsRequests.includes(null));
    assert.deepEqual(commentsRequests.filter(Boolean), [
      "comments + next",
      "comments + next",
    ]);
    await page.getByText("Capture 1", { exact: true }).waitFor();
    await page
      .getByRole("button", { name: "Load older captures", exact: true })
      .click();
    await page.getByText("Capture 2", { exact: true }).waitFor();
    assert.equal(await page.getByText("Capture 1", { exact: true }).count(), 1);
    assert.ok(revisitRequests.includes(null));
    assert.deepEqual(revisitRequests.filter(Boolean), ["revisits + next"]);

    failRevisits = true;
    await page.reload();
    await page.getByRole("heading", { name: "Sign 1", exact: true }).waitFor();
    await page
      .getByText("Capture history could not be loaded.", { exact: false })
      .waitFor();
    failRevisits = false;
    assert.equal(
      await page
        .getByRole("heading", { name: "Not Found", exact: true })
        .count(),
      0,
    );
    await page.getByRole("button", { name: "Try again", exact: true }).click();
    await page.getByText("Capture 1", { exact: true }).waitFor();

    await page.goto(`${origin}/community`);
    await page.getByRole("button", { name: "Challenges", exact: true }).click();
    await page
      .getByRole("heading", { name: "Challenge 50", exact: true })
      .waitFor();
    await page
      .getByRole("button", { name: "Load more challenges", exact: true })
      .click();
    await page
      .getByText("More challenges could not be loaded. Please retry.", {
        exact: true,
      })
      .waitFor();
    assert.equal(
      await page
        .getByRole("heading", { name: "Challenge 1", exact: true })
        .count(),
      1,
    );
    await page
      .getByRole("button", { name: "Load more challenges", exact: true })
      .click();
    await page
      .getByRole("heading", { name: "Challenge 52", exact: true })
      .waitFor();
    assert.equal(
      await page
        .getByRole("button", { name: "Load more challenges", exact: true })
        .count(),
      0,
    );
    assert.deepEqual(challengesRequests, [
      null,
      "challenges + next",
      "challenges + next",
    ]);
    assert.deepEqual(errors, []);
    await context.close();
    console.log(
      "Pagination controls, retry retention, and rolling-deployment compatibility passed",
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
