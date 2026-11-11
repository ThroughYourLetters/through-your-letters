const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium } = require("playwright");
const root = path.resolve(__dirname, "..");
const image =
  "data:image/svg+xml," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="800"><rect width="1200" height="800" fill="#f6e3dc"/><text x="80" y="400" font-size="80">Street lettering</text></svg>',
  );
const uuid = (n) => `00000000-0000-0000-0000-${String(n).padStart(12, "0")}`;
const user = (account) => ({
  id: uuid(account === "a" ? 1 : 2),
  email: `${account}@example.test`,
  display_name: account,
  role: "USER",
  created_at: "2026-11-01",
});
const lettering = (n, text) => ({
  id: uuid(n),
  city_id: uuid(1000),
  contributor_tag: "Walker",
  image_url: image,
  thumbnail_urls: { small: image, medium: image, large: image },
  location: null,
  location_source: "unresolved",
  pin_code: "560001",
  detected_text: text,
  ml_metadata: null,
  description: "Painted shop lettering",
  is_lettering: true,
  status: "APPROVED",
  likes_count: 0,
  comments_count: 0,
  cultural_context: null,
  created_at: "2026-11-01T00:00:00Z",
  updated_at: "2026-11-01T00:00:00Z",
  is_owner: true,
});
const original = lettering(100, "Original shop sign");
const target = lettering(101, "Repainted shop sign");
const collection = (n, account) => ({
  id: uuid(2000 + n),
  name:
    n === 22
      ? "AnUnbrokenCollectionName".repeat(12)
      : `${account.toUpperCase()} collection ${n}`,
  creator_tag: account,
  description: null,
  is_public: n !== 1,
  item_count: 0,
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
  let browser;
  try {
    await server.listen();
    const origin = `http://127.0.0.1:${server.httpServer.address().port}`;
    browser = await chromium.launch({ headless: true, channel: "chromium" });
    for (const width of [1280, 320]) {
      const context = await browser.newContext({
        viewport: { width, height: 800 },
        serviceWorkers: "block",
      });
      await context.addInitScript((account) => {
        sessionStorage.setItem("ttl_user_token", "a-token");
        sessionStorage.setItem(
          "ttl_user_account",
          JSON.stringify({ token: "a-token", user: account }),
        );
      }, user("a"));
      await context.route("https://fonts.googleapis.com/**", (route) =>
        route.abort(),
      );
      await context.routeWebSocket("**/ws/feed", () => {});
      let failCollections = true,
        failSave = true,
        failSearch = true,
        failLink = true;
      let holdSave = false,
        holdCollections = false;
      let saves = 0,
        links = 0,
        detailReads = 0;
      const held = [];
      let markStaleStarted, markStaleFinished;
      const staleStarted = new Promise((resolve) => {
        markStaleStarted = resolve;
      });
      const staleFinished = new Promise((resolve) => {
        markStaleFinished = resolve;
      });
      const collectionQueries = [];
      await context.route("**/api/v1/**", async (route) => {
        const request = route.request();
        const url = new URL(request.url());
        const account =
          request.headers().authorization === "Bearer b-token" ? "b" : "a";
        const fail = (message) =>
          route.fulfill({ status: 503, json: { error: message } });
        if (url.pathname === "/api/v1/auth/me")
          return route.fulfill({ json: user(account) });
        if (url.pathname === "/api/v1/auth/login") {
          const name = request.postDataJSON().email[0];
          return route.fulfill({
            json: { token: `${name}-token`, user: user(name) },
          });
        }
        if (url.pathname === "/api/v1/collections") {
          collectionQueries.push({
            mine: url.searchParams.get("mine"),
            offset: Number(url.searchParams.get("offset")),
            token: request.headers().authorization,
          });
          if (failCollections) return fail("Collections unavailable");
          if (holdCollections && account === "a")
            await new Promise((resolve) => held.push(resolve));
          const rows =
            account === "a"
              ? Array.from({ length: 23 }, (_, n) => collection(n + 1, "a"))
              : [collection(1, "b")];
          const offset = Number(url.searchParams.get("offset") || 0);
          return route
            .fulfill({
              json: {
                items: rows.slice(offset, offset + 20),
                total: rows.length,
                limit: 20,
                offset,
              },
            })
            .catch(() => {});
        }
        if (
          url.pathname.includes("/collections/") &&
          request.method() === "POST"
        ) {
          saves++;
          if (failSave) return fail("Saving is temporarily unavailable");
          if (holdSave) await new Promise((resolve) => held.push(resolve));
          return route.fulfill({ status: 201, body: "" });
        }
        if (url.pathname === "/api/v1/letterings/search") {
          if (url.searchParams.get("q") === "old") {
            markStaleStarted();
            await new Promise((resolve) => held.push(resolve));
            await route
              .fulfill({ json: [lettering(102, "Old search result")] })
              .catch(() => {});
            markStaleFinished();
            return;
          }
          if (failSearch) return fail("Search is temporarily unavailable");
          return route.fulfill({ json: [original, target] });
        }
        if (url.pathname.endsWith("/revisits")) {
          if (request.method() === "POST") {
            links++;
            if (failLink) return fail("Linking is temporarily unavailable");
            assert.deepEqual(request.postDataJSON(), {
              revisit_lettering_id: target.id,
              notes: "Repainted in November",
            });
            return route.fulfill({ status: 201, body: "" });
          }
          return route.fulfill({
            json: {
              revisits:
                links >= 2
                  ? [
                      {
                        id: uuid(3000),
                        original_lettering_id: original.id,
                        revisit_lettering_id: target.id,
                        notes: "Repainted in November",
                        created_at: target.created_at,
                        original: {
                          image_url: image,
                          created_at: original.created_at,
                        },
                        revisit: {
                          image_url: image,
                          created_at: target.created_at,
                        },
                      },
                    ]
                  : [],
              next_cursor: null,
            },
          });
        }
        if (url.pathname === `/api/v1/letterings/${original.id}`) {
          detailReads++;
          return route.fulfill({
            json: {
              ...original,
              is_owner: request.headers().authorization === "Bearer a-token",
            },
          });
        }
        if (url.pathname.endsWith("/similar"))
          return route.fulfill({ json: { similar: [] } });
        if (
          url.pathname === "/api/v1/cities" ||
          url.pathname.endsWith("/comments")
        )
          return route.fulfill({ json: [] });
        return route.fulfill({ json: { letterings: [], items: [], total: 0 } });
      });
      const page = await context.newPage();
      page.setDefaultTimeout(12_000);
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      try {
        await page.goto(`${origin}/lettering/${original.id}`);
        await page
          .getByRole("heading", { name: original.detected_text, exact: true })
          .waitFor();
        const initialReads = detailReads;
        const save = page.getByRole("button", { name: "Save", exact: true });
        await save.click();
        let dialog = page.getByRole("dialog", {
          name: "Save to a collection",
          exact: true,
        });
        await dialog
          .getByRole("alert")
          .getByText("Your collections could not be loaded.")
          .waitFor();
        assert.equal(await dialog.getByText(/no collections yet/).count(), 0);
        failCollections = false;
        await dialog
          .getByRole("button", { name: "Try again", exact: true })
          .click();
        await dialog
          .getByRole("button", { name: "A collection 1 Private", exact: true })
          .waitFor();
        for (let n = 0; n < 25; n++) {
          await page.keyboard.press("Tab");
          // Browser chrome remains reachable; background controls must stay inert.
          assert.equal(
            await dialog.evaluate(
              (d) => !document.hasFocus() || d.contains(document.activeElement),
            ),
            true,
            "Tab cannot focus background controls",
          );
        }
        await dialog
          .getByRole("button", { name: "More collections", exact: true })
          .click();
        await dialog
          .getByRole("button", { name: "A collection 23", exact: true })
          .waitFor();
        assert.equal(
          await dialog.evaluate((d) => d.scrollWidth <= d.clientWidth),
          true,
          "Long collection names fit the dialog",
        );
        await page.screenshot({
          path: path.join(
            require("node:os").tmpdir(),
            `ttl-collection-dialog-${width}.png`,
          ),
        });
        assert.ok(collectionQueries.some((q) => q.offset === 20));
        assert.ok(
          collectionQueries.every(
            (q) => q.mine === "true" && q.token === "Bearer a-token",
          ),
        );
        await dialog
          .getByRole("button", { name: "A collection 23", exact: true })
          .click();
        await dialog
          .getByRole("alert")
          .getByText("Saving is temporarily unavailable")
          .waitFor();
        assert.equal(
          await dialog
            .getByRole("button", {
              name: "A collection 1 Private",
              exact: true,
            })
            .count(),
          1,
        );
        failSave = false;
        holdSave = true;
        const beforeSaves = saves;
        await dialog
          .getByRole("button", { name: "A collection 23", exact: true })
          .evaluate((button) => {
            button.click();
            button.click();
          });
        await page.waitForFunction(() =>
          [...document.querySelectorAll("dialog button")].some((b) =>
            b.textContent.includes("Saving…"),
          ),
        );
        assert.equal(saves, beforeSaves + 1);
        held.splice(0).forEach((resolve) => resolve());
        holdSave = false;
        await dialog.waitFor({ state: "detached" });
        assert.equal(
          await save.evaluate((button) => button === document.activeElement),
          true,
          "Saving restores the opener's focus",
        );

        await save.click();
        await page.getByRole("dialog").waitFor();
        await page.keyboard.press("Escape");
        await page.getByRole("dialog").waitFor({ state: "detached" });
        assert.equal(
          await save.evaluate((button) => button === document.activeElement),
          true,
        );
        await save.click();
        await page.getByRole("dialog").waitFor();
        await page.mouse.click(2, 2);
        await page.getByRole("dialog").waitFor({ state: "detached" });

        const openImage = page.getByRole("button", {
          name: "View full size",
          exact: true,
        });
        await page.evaluate(() => {
          document.body.style.overflow = "scroll";
        });
        await openImage.click();
        dialog = page.getByRole("dialog");
        await dialog
          .getByRole("button", { name: "Zoom in", exact: true })
          .click();
        await page.keyboard.press("+");
        assert.equal(
          await dialog.getByRole("img").evaluate((img) => img.style.transform),
          "scale(1.5)",
        );
        assert.equal(
          await page.evaluate(
            () => getComputedStyle(document.documentElement).overflow,
          ),
          "hidden",
        );
        await page.keyboard.press("Escape");
        await dialog.waitFor({ state: "detached" });
        assert.equal(
          await openImage.evaluate(
            (button) => button === document.activeElement,
          ),
          true,
        );
        assert.equal(
          await page.evaluate(() => document.body.style.overflow),
          "scroll",
        );

        const link = page.getByRole("button", {
          name: "Link a revisit",
          exact: true,
        });
        await link.click();
        dialog = page.getByRole("dialog", {
          name: "Link a timeline revisit",
          exact: true,
        });
        const search = dialog.getByRole("textbox", {
          name: "Search by text or postal code",
          exact: true,
        });
        await search.fill("painted");
        await search.press("Enter");
        await dialog
          .getByRole("alert")
          .getByText("Search is temporarily unavailable")
          .waitFor();
        failSearch = false;
        await search.fill("old");
        await search.press("Enter");
        await staleStarted;
        await search.fill("painted");
        await search.press("Enter");
        const result = dialog.getByRole("button", {
          name: /Repainted shop sign/,
        });
        await result.waitFor();
        held.splice(0).forEach((resolve) => resolve());
        await staleFinished;
        assert.equal(
          await dialog.getByText("Old search result", { exact: true }).count(),
          0,
        );
        assert.equal(
          await dialog
            .getByText(original.detected_text, { exact: true })
            .count(),
          0,
        );
        await dialog
          .getByLabel("What changed? (optional)", { exact: true })
          .fill("Repainted in November");
        await result.click();
        await dialog
          .getByRole("alert")
          .getByText("Linking is temporarily unavailable")
          .waitFor();
        assert.equal(
          await dialog
            .getByLabel("What changed? (optional)", { exact: true })
            .inputValue(),
          "Repainted in November",
        );
        failLink = false;
        const beforeLinks = links;
        await result.evaluate((button) => {
          button.click();
          button.click();
        });
        await dialog.waitFor({ state: "detached" });
        assert.equal(links, beforeLinks + 1);
        await page
          .getByText("Repainted in November", { exact: true })
          .waitFor();
        const comparison = page.getByRole("slider", {
          name: "Compare original and revisit images",
          exact: true,
        });
        await comparison.focus();
        await page.keyboard.press("ArrowRight");
        assert.equal(await comparison.inputValue(), "51");
        assert.equal(
          detailReads,
          initialReads,
          "Closing dialogs does not reload the detail page",
        );
        assert.equal(
          await page.evaluate(
            () => document.documentElement.scrollWidth <= innerWidth,
          ),
          true,
          `No overflow at ${width}px`,
        );
        await page.screenshot({
          path: path.join(
            require("node:os").tmpdir(),
            `ttl-dialogs-${width}.png`,
          ),
          fullPage: true,
        });

        holdCollections = true;
        await save.click();
        await page.getByRole("dialog").waitFor();
        await page.evaluate(async () => {
          const { useAuthStore } = await import("/src/store/useAuthStore.ts");
          useAuthStore.getState().logout();
          await useAuthStore.getState().login("b@example.test", "fixture");
        });
        await page.getByRole("dialog").waitFor({ state: "detached" });
        held.splice(0).forEach((resolve) => resolve());
        holdCollections = false;
        await page.getByRole("button", { name: "Save", exact: true }).click();
        dialog = page.getByRole("dialog");
        await dialog
          .getByRole("button", { name: "B collection 1 Private", exact: true })
          .waitFor();
        assert.equal(await dialog.getByText(/^A collection/).count(), 0);
        await page.keyboard.press("Escape");
        await page.evaluate(async () =>
          (await import("/src/store/useAuthStore.ts")).useAuthStore
            .getState()
            .logout(),
        );
        await page
          .getByRole("button", { name: "Link a revisit", exact: true })
          .waitFor({ state: "detached" });
        await page.getByRole("button", { name: "Save", exact: true }).click();
        await page.waitForURL("**/auth");
        assert.deepEqual(
          errors,
          [],
          "No unhandled errors from failed or cancelled requests",
        );
        console.log(
          `Dialog keyboard, failures, pagination, focus, and account isolation passed at ${width}px.`,
        );
      } finally {
        held.splice(0).forEach((resolve) => resolve());
        await context.close();
      }
    }
  } finally {
    await browser?.close();
    await server.close();
    process.chdir(previous);
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
