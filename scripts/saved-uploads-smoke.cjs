const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium } = require("playwright");
const root = path.resolve(__dirname, "..");

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
    const context = await browser.newContext({
      serviceWorkers: "block",
      acceptDownloads: true,
    });
    await context.addInitScript(() => {
      window.fixtureOnline = false;
      Object.defineProperty(navigator, "onLine", {
        configurable: true,
        get: () => window.fixtureOnline,
      });
      const user = {
        id: "a",
        email: "a@example.test",
        display_name: "A",
        role: "USER",
        created_at: "2026-01-01",
      };
      sessionStorage.setItem("ttl_user_token", "token-a");
      sessionStorage.setItem(
        "ttl_user_account",
        JSON.stringify({ token: "token-a", user }),
      );
    });
    await context.route("https://fonts.googleapis.com/**", (route) =>
      route.abort(),
    );
    await context.routeWebSocket("**/ws/feed", () => {});
    let status = 400;
    const attempts = [];
    await context.route("**/api/v1/**", (route) => {
      const request = route.request();
      const pathname = new URL(request.url()).pathname;
      if (pathname === "/api/v1/auth/me")
        return route.fulfill({
          json: {
            id: "a",
            email: "a@example.test",
            display_name: "A",
            role: "USER",
            created_at: "2026-01-01",
          },
        });
      if (pathname === "/api/v1/letterings/upload") {
        attempts.push({
          key: request.headers()["idempotency-key"],
          body: request.postDataBuffer(),
        });
        return route.fulfill({
          status,
          headers:
            status === 429
              ? {
                  "Retry-After": "60",
                  "Access-Control-Expose-Headers": "Retry-After",
                }
              : {},
          json:
            status === 200
              ? { id: "uploaded" }
              : { error: "Fixture rejection" },
        });
      }
      if (pathname === "/api/v1/cities") return route.fulfill({ json: [] });
      return route.fulfill({
        json: { items: [], letterings: [], total: 0, unread_count: 0 },
      });
    });
    const page = await context.newPage();
    page.setDefaultTimeout(10_000);
    const browserErrors = [];
    page.on("pageerror", (error) => {
      browserErrors.push(error.message);
      console.error("Browser:", error.message);
    });
    page.on("dialog", (dialog) => dialog.accept());
    await page.goto(`${origin}/me/queue`);
    await page
      .getByRole("heading", { name: "Saved uploads", exact: true })
      .waitFor();
    const save = (name) =>
      page.evaluate(async (name) => {
        const queue = await import("/src/lib/offlineQueue.ts");
        await queue.enqueueUpload({
          imageBlob: new Blob(["private-photo"]),
          fileName: name,
          contributorTag: "A",
          pinCode: "560001",
          description: "Private draft",
          cityId: "city",
        });
      }, name);
    const row = (name) =>
      page
        .getByRole("listitem")
        .filter({ has: page.getByRole("heading", { name, exact: true }) });
    const online = (value) =>
      page.evaluate((value) => {
        window.fixtureOnline = value;
      }, value);
    const switchAccount = (id) =>
      page.evaluate(async (id) => {
        window.fixtureOnline = false;
        const session = await import("/src/lib/userSession.ts");
        const { useAuthStore } = await import("/src/store/useAuthStore.ts");
        const user = {
          id,
          email: `${id}@example.test`,
          display_name: id,
          role: "USER",
          created_at: "2026-01-01",
        };
        const revision = session.replaceUserSession(`token-${id}`, user);
        useAuthStore.setState({
          user,
          token: `token-${id}`,
          revision,
          hydrated: true,
          loading: false,
        });
      }, id);

    await save("invalid.png");
    await row("invalid.png").waitFor();
    await online(true);
    await row("invalid.png")
      .getByRole("button", { name: "Retry upload" })
      .click();
    await row("invalid.png").getByText("Failed", { exact: true }).waitFor();
    await row("invalid.png")
      .getByText(/submission was not accepted/)
      .waitFor();
    const download = page.waitForEvent("download");
    await row("invalid.png")
      .getByRole("button", { name: "Download photo" })
      .click();
    assert.equal((await download).suggestedFilename(), "invalid.png");
    await row("invalid.png")
      .getByRole("button", { name: "Discard", exact: true })
      .click();
    await row("invalid.png").waitFor({ state: "detached" });

    await online(false);
    await save("temporary.png");
    status = 503;
    await online(true);
    await row("temporary.png")
      .getByRole("button", { name: "Retry upload" })
      .click();
    await row("temporary.png").getByText("Blocked", { exact: true }).waitFor();
    status = 200;
    await row("temporary.png")
      .getByRole("button", { name: "Retry upload" })
      .click();
    await row("temporary.png").waitFor({ state: "detached" });
    assert.equal(attempts.at(-1).key, attempts.at(-2).key);
    assert.ok(attempts.at(-1).body.includes(Buffer.from("private-photo")));

    await online(false);
    await save("limited.png");
    status = 429;
    await online(true);
    await row("limited.png")
      .getByRole("button", { name: "Retry upload" })
      .click();
    await row("limited.png")
      .getByText(/Retry after/)
      .waitFor();
    assert.equal(
      await row("limited.png")
        .getByRole("button", { name: "Retry upload" })
        .isDisabled(),
      true,
    );
    await row("limited.png")
      .getByRole("button", { name: "Discard", exact: true })
      .click();
    await row("limited.png").waitFor({ state: "detached" });

    await online(false);
    await save("expired.png");
    status = 401;
    await online(true);
    await row("expired.png")
      .getByRole("button", { name: "Retry upload" })
      .click();
    await page.getByText(/Your session expired/).waitFor();
    await page
      .locator("main")
      .getByRole("link", { name: "Sign in", exact: true })
      .waitFor();
    assert.equal(
      await page.evaluate(() => sessionStorage.getItem("ttl_user_token")),
      null,
    );
    assert.equal(
      await page.getByText("expired.png", { exact: true }).count(),
      0,
    );
    await switchAccount("b");
    await page.getByText("No saved photos on this page.").waitFor();
    await save("b-private.png");
    await row("b-private.png").waitFor();
    assert.equal(
      await page.getByText("expired.png", { exact: true }).count(),
      0,
    );
    await switchAccount("a");
    await row("expired.png").waitFor();
    assert.equal(
      await page.getByText("b-private.png", { exact: true }).count(),
      0,
    );
    await page.evaluate(async () => {
      const queue = await import("/src/lib/offlineQueue.ts");
      for (let i = 0; i < 20; i++)
        await queue.enqueueUpload({
          imageBlob: new Blob(["fixture"]),
          fileName: `page-${i}.png`,
          contributorTag: "A",
          pinCode: "560001",
          description: "",
          cityId: "city",
        });
    });
    await page.getByRole("button", { name: "Next page", exact: true }).click();
    await row("page-19.png").waitFor();
    assert.equal(await page.getByRole("listitem").count(), 1);
    await page
      .getByRole("button", { name: "Previous page", exact: true })
      .click();
    await row("expired.png").waitFor();

    // A full queue must not navigate away from the unsaved remainder of a batch.
    await page.evaluate(async () => {
      const queue = await import("/src/lib/offlineQueue.ts");
      const { webLimits } = await import("/src/lib/runtimeConfig.ts");
      for (
        let i = await queue.getQueuedCount();
        i < webLimits.offline_max_entries - 1;
        i++
      )
        await queue.enqueueUpload({
          imageBlob: new Blob(["fixture"]),
          fileName: `fill-${i}.png`,
          contributorTag: "A",
          pinCode: "560001",
          description: "",
          cityId: "city",
        });
      const { useCityStore } = await import("/src/store/useCityStore.ts");
      useCityStore.setState({ selectedCityId: "city" });
    });
    await page.getByRole("link", { name: "Add photos", exact: true }).click();
    await page
      .locator('input[type="file"]')
      .first()
      .setInputFiles([
        {
          name: "first.png",
          mimeType: "image/png",
          buffer: Buffer.from("fixture"),
        },
        {
          name: "second.png",
          mimeType: "image/png",
          buffer: Buffer.from("fixture"),
        },
      ]);
    await page.getByPlaceholder("Contributor Name").fill("Tester");
    await page
      .getByRole("textbox", { name: "Postal or ZIP code" })
      .fill("560001");
    await page
      .getByRole("button", { name: "Save for later", exact: true })
      .click();
    await page.getByText(/Some photos could not be saved/).waitFor();
    assert.equal(new URL(page.url()).pathname, "/contribute");
    assert.equal(
      await page.getByText("Saved for later", { exact: true }).count(),
      1,
    );
    assert.equal(
      await page.evaluate(async () =>
        (await import("/src/lib/offlineQueue.ts")).getQueuedCount(),
      ),
      50,
    );
    assert.deepEqual(browserErrors, []);
    console.log(
      "Saved-upload UI handles failed requests, retry, download, discard, rate limits, sessions, account isolation, pagination, and partial saves.",
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
