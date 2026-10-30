const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium } = require("playwright");

const root = path.resolve(__dirname, "..");
const image = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a/S8AAAAASUVORK5CYII=",
  "base64",
);
const account = {
  id: "postal-user",
  email: "postal@example.test",
  display_name: "Postal Tester",
  role: "USER",
  created_at: "2026-01-01",
};
const item = {
  id: "postal-upload",
  image_url: `data:image/png;base64,${image.toString("base64")}`,
  thumbnail_small: "",
  pin_code: "560001",
  contributor_tag: "Postal Tester",
  detected_text: null,
  description: "Postal fixture",
  status: "APPROVED",
  likes_count: 0,
  comments_count: 0,
  report_count: 0,
  moderation_reason: null,
  moderated_at: null,
  moderated_by: null,
  created_at: "2026-01-01",
  updated_at: "2026-01-01",
};

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
    const context = await browser.newContext({ serviceWorkers: "block" });
    await context.route("https://fonts.googleapis.com/**", (route) =>
      route.abort(),
    );
    await context.routeWebSocket("**/ws/feed", () => {});
    const uploads = [];
    const edits = [];
    await context.route("**/api/v1/**", async (route) => {
      const request = route.request();
      const url = new URL(request.url());
      if (url.pathname.endsWith("/auth/login"))
        return route.fulfill({
          json: { token: "postal-token", user: account },
        });
      if (url.pathname.endsWith("/auth/me"))
        return route.fulfill({ json: account });
      if (url.pathname.endsWith("/letterings/upload")) {
        uploads.push(request.postDataBuffer().toString("utf8"));
        return route.fulfill({
          json: { id: "created", location_source: "postal_code" },
        });
      }
      if (request.method() === "PATCH") {
        edits.push(request.postDataJSON());
        Object.assign(item, request.postDataJSON());
        return route.fulfill({ json: item });
      }
      if (url.pathname.endsWith("/me/letterings"))
        return route.fulfill({
          json: { items: [item], total: 1, limit: 50, offset: 0 },
        });
      return route.fulfill({
        json:
          url.pathname === "/api/v1/letterings"
            ? { letterings: [], total: 0 }
            : [],
      });
    });
    const page = await context.newPage();
    page.setDefaultTimeout(20000);
    await page.goto(`${origin}/contribute`);
    await page.evaluate(async () => {
      const { useAuthStore } = await import("/src/store/useAuthStore.ts");
      await useAuthStore.getState().login("postal@example.test", "fixture");
      const { useCityStore } = await import("/src/store/useCityStore.ts");
      useCityStore
        .getState()
        .setCity(
          "0194f123-4567-7abc-8def-0123456789ab",
          "London",
          51.5,
          -0.1,
          10,
        );
    });
    const attach = async () => {
      await page.locator('input[type="file"]').first().setInputFiles({
        name: "postal.png",
        mimeType: "image/png",
        buffer: image,
      });
      await page.getByPlaceholder("Contributor Name").fill("Postal Tester");
    };
    await attach();
    const tooLong = "𐐀".repeat(21);
    await page
      .getByRole("textbox", { name: "Postal or ZIP code" })
      .fill(tooLong);
    await page
      .getByRole("button", { name: "Upload photo", exact: true })
      .click();
    await page
      .getByText("Postal code must be 1 to 20 characters.", { exact: true })
      .waitFor();
    assert.equal(uploads.length, 0);
    assert.equal(
      await page
        .getByRole("textbox", { name: "Postal or ZIP code" })
        .inputValue(),
      tooLong,
    );

    const unicode = "𐐀".repeat(20);
    await page
      .getByRole("textbox", { name: "Postal or ZIP code" })
      .fill(`  ${unicode}  `);
    await page
      .getByRole("button", { name: "Upload photo", exact: true })
      .click();
    await page.waitForURL(`${origin}/`);
    assert.equal(uploads.length, 1);
    assert.ok(uploads[0].includes(`name="pin_code"\r\n\r\n${unicode}\r\n`));

    await page.goto(`${origin}/me/uploads`);
    await page.getByRole("button", { name: "Edit", exact: true }).click();
    const postal = page.getByRole("textbox", { name: "Postal or ZIP code" });
    await postal.fill(tooLong);
    assert.equal(await postal.inputValue(), tooLong);
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await page
      .getByText("Postal code must be 1 to 20 characters.", { exact: true })
      .waitFor();
    assert.equal(edits.length, 0);
    await postal.fill("\u0085 SW1A 1AA \u2003");
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await page.getByRole("button", { name: "Edit", exact: true }).waitFor();
    assert.equal(edits[0].pin_code, "SW1A 1AA");

    await page.goto(`${origin}/contribute`);
    await page.evaluate(async () => {
      const { useCityStore } = await import("/src/store/useCityStore.ts");
      useCityStore
        .getState()
        .setCity(
          "0194f123-4567-7abc-8def-0123456789ab",
          "London",
          51.5,
          -0.1,
          10,
        );
      Object.defineProperty(navigator, "onLine", {
        configurable: true,
        get: () => false,
      });
    });
    await attach();
    await page
      .getByRole("textbox", { name: "Postal or ZIP code" })
      .fill(tooLong);
    await page
      .getByRole("button", { name: "Upload photo", exact: true })
      .click();
    await page
      .getByText("Postal code must be 1 to 20 characters.", { exact: true })
      .waitFor();
    const queued = () =>
      page.evaluate(async () =>
        (await import("/src/lib/offlineQueue.ts")).getQueuedCount(),
      );
    assert.equal(await queued(), 0);
    await page
      .getByRole("textbox", { name: "Postal or ZIP code" })
      .fill("\u0085 K1A 0B1 \u2003");
    await page
      .getByRole("button", { name: "Upload photo", exact: true })
      .click();
    await page.waitForURL(`${origin}/`);
    assert.equal(await queued(), 1);
    const startupReplay = page.waitForResponse(
      (response) =>
        response.url().endsWith("/api/v1/letterings/upload") &&
        response.request().method() === "POST",
    );
    await page.reload();
    assert.equal((await startupReplay).status(), 200);
    await page.waitForFunction(async () =>
      (await import("/src/lib/offlineQueue.ts"))
        .getQueuedCount()
        .then((count) => count === 0),
    );
    assert.equal(await queued(), 0);
    assert.equal(uploads.length, 2);
    assert.ok(uploads[1].includes('name="pin_code"\r\n\r\nK1A 0B1\r\n'));
    const reconnectReplay = page.waitForResponse(
      (response) =>
        response.url().endsWith("/api/v1/letterings/upload") &&
        response.request().method() === "POST",
    );
    await page.evaluate(async (bytes) => {
      const queue = await import("/src/lib/offlineQueue.ts");
      await queue.enqueueUpload({
        imageBlob: new Blob([new Uint8Array(bytes)], { type: "image/png" }),
        fileName: "reconnect.png",
        contributorTag: "Postal Tester",
        pinCode: "SW1A 1AA",
        description: "Reconnect",
        cityId: "0194f123-4567-7abc-8def-0123456789ab",
      });
      window.dispatchEvent(new Event("online"));
    }, Array.from(image));
    assert.equal((await reconnectReplay).status(), 200);
    await page.waitForFunction(async () =>
      (await import("/src/lib/offlineQueue.ts"))
        .getQueuedCount()
        .then((count) => count === 0),
    );
    assert.equal(uploads.length, 3);
    console.log(
      "Postal upload/edit validation, startup replay, and reconnect replay passed without truncation",
    );
  } finally {
    await browser?.close();
    await server.close();
    process.chdir(previous);
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
