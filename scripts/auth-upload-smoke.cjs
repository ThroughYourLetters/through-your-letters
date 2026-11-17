const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium } = require("playwright");
const root = path.resolve(__dirname, "..");
const user = {
  id: "forms-user",
  email: "forms@example.test",
  display_name: "Forms",
  role: "USER",
  created_at: "2026-11-01",
};
const image = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a/S8AAAAASUVORK5CYII=",
  "base64",
);
const until = async (condition) => {
  const deadline = Date.now() + 10000;
  while (!condition() && Date.now() < deadline)
    await new Promise((resolve) => setTimeout(resolve, 20));
  assert.ok(condition(), "Expected intercepted request was not received");
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
    for (const width of [1280, 320]) {
      const context = await browser.newContext({
        viewport: { width, height: 800 },
        serviceWorkers: "block",
      });
      await context.route("https://fonts.googleapis.com/**", (route) =>
        route.abort(),
      );
      await context.routeWebSocket("**/ws/feed", () => {});
      const requests = [];
      const held = [];
      const policy = {};
      const browserErrors = [];
      await context.route("**/api/v1/**", async (route) => {
        const pathname = new URL(route.request().url()).pathname;
        if (route.request().method() === "POST")
          requests.push({
            path: pathname,
            body: pathname.includes("/auth/")
              ? route.request().postDataJSON()
              : undefined,
            key: route.request().headers()["idempotency-key"],
          });
        const result = policy[pathname];
        if (result === "hold") {
          held.push({ path: pathname, route });
          return;
        }
        if (result) return route.fulfill(result);
        if (pathname === "/api/v1/auth/me")
          return route.fulfill({ json: user });
        if (
          pathname === "/api/v1/auth/login" ||
          pathname === "/api/v1/auth/register"
        )
          return route.fulfill({ json: { token: "forms-token", user } });
        return route.fulfill({
          json:
            pathname === "/api/v1/cities"
              ? []
              : { items: [], letterings: [], total: 0, unread_count: 0 },
        });
      });
      let directGeocoderRequests = 0;
      await context.route("https://nominatim.openstreetmap.org/**", (route) => {
        directGeocoderRequests++;
        return route.abort();
      });
      await context.route("https://*.tile.openstreetmap.org/**", (route) =>
        route.abort(),
      );
      const page = await context.newPage();
      page.setDefaultTimeout(10000);
      page.on("pageerror", (error) => browserErrors.push(error.message));
      const form = (name) => page.getByRole("form", { name, exact: true });
      const count = (endpoint) =>
        requests.filter((r) => r.path === `/api/v1/${endpoint}`).length;
      const assertFits = async () =>
        assert.ok(
          await page.evaluate(
            () =>
              document.documentElement.scrollWidth <=
              document.documentElement.clientWidth,
          ),
          `Horizontal overflow at ${width}px`,
        );
      const fail = (message, status = 503) => ({
        status,
        json: { error: message },
      });
      await page.goto(`${origin}/auth`);
      policy["/api/v1/auth/login"] = fail(
        "Email or password is incorrect",
        401,
      );
      await page.getByLabel("Email", { exact: true }).fill(user.email);
      await page.getByLabel("Password", { exact: true }).fill("legacy");
      await form("Sign In")
        .getByRole("button", { name: "Sign In", exact: true })
        .click();
      const loginError = form("Sign In").getByRole("alert");
      await loginError.waitFor();
      assert.equal(
        await loginError.textContent(),
        "Email or password is incorrect",
      );
      assert.equal(
        await loginError.evaluate((node) => node === document.activeElement),
        true,
      );
      assert.equal(
        await page.getByLabel("Password", { exact: true }).inputValue(),
        "legacy",
      );
      await assertFits();
      await page.screenshot({
        path: `/tmp/ttl-auth-form-${width}.png`,
        fullPage: true,
      });

      // A cancelled request must stay cancelled even if the transport ignores abort.
      await page.evaluate(() => {
        const original = window.fetch;
        window.fetch = (url, init) =>
          original(url, { ...init, signal: undefined });
      });
      policy["/api/v1/auth/login"] = "hold";
      const before = count("auth/login");
      await page.locator("form").evaluate((node) => {
        node.requestSubmit();
        node.requestSubmit();
      });
      await until(() => held.length === 1);
      assert.equal(count("auth/login"), before + 1);
      await page.getByRole("button", { name: "Register", exact: true }).click();
      const registerHeading = page.getByRole("heading", {
        name: "Create Account",
        exact: true,
      });
      assert.equal(
        await registerHeading.evaluate(
          (node) => node === document.activeElement,
        ),
        true,
      );
      const stale = held.shift();
      const staleResponse = page.waitForResponse((response) =>
        response.url().endsWith("/auth/login"),
      );
      await stale.route.fulfill({ json: { token: "stale-token", user } });
      await staleResponse;
      await page.waitForLoadState("networkidle");
      assert.equal(
        await page.evaluate(() => sessionStorage.getItem("ttl_user_token")),
        null,
      );
      assert.equal(new URL(page.url()).pathname, "/auth");
      await page
        .getByLabel("Display name (optional)", { exact: true })
        .fill("Forms Tester");
      await page.getByLabel("Password", { exact: true }).fill("short");
      await form("Create Account")
        .getByRole("button", { name: "Create Account", exact: true })
        .click();
      assert.equal(
        await page
          .getByLabel("Password", { exact: true })
          .evaluate((node) => node.validity.tooShort),
        true,
      );
      assert.equal(count("auth/register"), 0);
      await page.getByLabel("Password", { exact: true }).fill("new-password");
      policy["/api/v1/auth/register"] = fail(
        "Registration is temporarily unavailable",
      );
      await form("Create Account")
        .getByRole("button", { name: "Create Account", exact: true })
        .click();
      await form("Create Account").getByRole("alert").waitFor();
      assert.equal(
        await page.getByLabel("Display name (optional)").inputValue(),
        "Forms Tester",
      );
      delete policy["/api/v1/auth/register"];
      await form("Create Account")
        .getByRole("button", { name: "Create Account", exact: true })
        .click();
      await page.waitForURL(`${origin}/me/uploads`);
      await page.evaluate(async () =>
        (await import("/src/store/useAuthStore.ts")).useAuthStore
          .getState()
          .logout(),
      );

      await page.goto(`${origin}/auth`);
      await page
        .getByRole("button", { name: "Forgot Password?", exact: true })
        .click();
      await page.getByLabel("Email", { exact: true }).fill(user.email);
      policy["/api/v1/auth/forgot-password"] = fail(
        "Email service is temporarily unavailable",
      );
      await page
        .getByRole("button", { name: "Send Reset Link", exact: true })
        .click();
      await form("Forgot Password").getByRole("alert").waitFor();
      assert.equal(
        await page.getByLabel("Email", { exact: true }).inputValue(),
        user.email,
      );
      policy["/api/v1/auth/forgot-password"] = {
        json: { message: "If the account exists, an email was sent" },
      };
      await page
        .getByRole("button", { name: "Send Reset Link", exact: true })
        .click();
      await page
        .getByRole("heading", { name: "Check Your Email", exact: true })
        .waitFor();
      await page
        .getByRole("status")
        .filter({ hasText: "If an account" })
        .waitFor();
      await page
        .getByRole("button", { name: "Back to Sign In", exact: true })
        .click();
      await page
        .getByRole("button", { name: "Forgot Password?", exact: true })
        .click();
      await page
        .getByLabel("Email", { exact: true })
        .fill("unknown@example.test");
      await page
        .getByRole("button", { name: "Send Reset Link", exact: true })
        .click();
      await page
        .getByRole("status")
        .filter({ hasText: "If an account" })
        .waitFor();

      await page.goto(`${origin}/auth?token=cancelled-reset`);
      await page
        .getByRole("button", { name: "Back to Sign In", exact: true })
        .click();
      assert.equal(new URL(page.url()).searchParams.has("token"), false);
      await page.reload();
      await page
        .getByRole("heading", { name: "Sign In", exact: true })
        .waitFor();
      delete policy["/api/v1/auth/login"];
      await page.getByLabel("Email", { exact: true }).fill(user.email);
      await page.getByLabel("Password", { exact: true }).fill("legacy");
      await form("Sign In")
        .getByRole("button", { name: "Sign In", exact: true })
        .click();
      await page.waitForURL(`${origin}/me/uploads`);
      await page.goto(`${origin}/auth?token=reset-code&source=email#recovery`);
      await page
        .getByRole("heading", { name: "Reset Password", exact: true })
        .waitFor();
      assert.equal(
        await page.getByLabel("Reset token", { exact: true }).inputValue(),
        "reset-code",
      );
      await page
        .getByLabel("New password", { exact: true })
        .fill("replacement-password");
      policy["/api/v1/auth/reset-password"] = fail(
        "Invalid or expired reset token",
        400,
      );
      await page
        .getByRole("button", { name: "Update Password", exact: true })
        .click();
      await form("Reset Password").getByRole("alert").waitFor();
      assert.equal(
        await page.getByLabel("New password", { exact: true }).inputValue(),
        "replacement-password",
      );
      assert.equal(new URL(page.url()).searchParams.get("token"), "reset-code");
      policy["/api/v1/auth/reset-password"] = {
        json: { message: "Password updated successfully." },
      };
      await page
        .getByRole("button", { name: "Update Password", exact: true })
        .click();
      await page
        .getByRole("heading", { name: "Sign In", exact: true })
        .waitFor();
      await page
        .getByRole("status")
        .filter({ hasText: "Password updated." })
        .waitFor();
      assert.equal(
        await page.evaluate(() => sessionStorage.getItem("ttl_user_token")),
        null,
      );
      assert.equal(new URL(page.url()).search, "?source=email");
      assert.equal(new URL(page.url()).hash, "#recovery");

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
        Object.defineProperty(navigator, "geolocation", {
          configurable: true,
          value: {
            getCurrentPosition: (success, error) => {
              window.geoSuccess = success;
              window.geoError = error;
            },
          },
        });
      });
      const choose = page.getByRole("button", {
        name: "Choose photos",
        exact: true,
      });
      // Register interception before the focus round trip and Enter key.
      const chooserPromise = page.waitForEvent("filechooser");
      await choose.focus();
      await page.keyboard.press("Enter");
      const chooser = await chooserPromise;
      await chooser.setFiles({
        name: "lettering.png",
        mimeType: "image/png",
        buffer: image,
      });
      const remove = page.getByRole("button", {
        name: "Remove lettering.png",
        exact: true,
      });
      await remove.waitFor();
      assert.equal(await remove.count(), 1);
      const name = page.getByLabel("Contributor name", { exact: true });
      const postal = page.getByRole("textbox", {
        name: "Postal or ZIP code",
        exact: true,
      });
      const description = page.getByLabel("Description (optional)", {
        exact: true,
      });
      await page
        .getByRole("button", { name: "Choose photo location", exact: true })
        .click();
      const picker = page.getByRole("dialog", {
        name: "Choose photo location",
      });
      await picker
        .getByRole("button", { name: "Use current location", exact: true })
        .click();
      await page.evaluate(() => window.geoError({ code: 1 }));
      await picker
        .getByRole("alert")
        .filter({ hasText: "Location access denied" })
        .waitFor();
      await assertFits();
      await picker.getByRole("button", { name: "Cancel", exact: true }).click();
      await postal.fill("SW1A 1AA");
      assert.equal(await page.getByRole("alert").count(), 0);
      await page
        .getByRole("button", { name: "Choose photo location", exact: true })
        .click();
      await picker
        .getByRole("button", { name: "Use current location", exact: true })
        .click();
      await page.evaluate(() =>
        window.geoSuccess({ coords: { latitude: 51.5, longitude: -0.1 } }),
      );
      await picker
        .getByRole("button", { name: "Use current location", exact: true })
        .waitFor();
      assert.equal(
        await picker
          .getByRole("button", { name: "Confirm location" })
          .isDisabled(),
        true,
      );
      await picker.getByRole("button", { name: "Cancel", exact: true }).click();
      assert.equal(await postal.inputValue(), "SW1A 1AA");
      await page
        .getByRole("button", { name: "Choose photo location", exact: true })
        .click();
      await picker
        .getByRole("button", { name: "Use current location", exact: true })
        .click();
      await picker.getByRole("button", { name: "Cancel", exact: true }).click();
      await postal.fill("10001");
      await page.evaluate(() =>
        window.geoSuccess({ coords: { latitude: 1, longitude: 2 } }),
      );
      assert.equal(await postal.inputValue(), "10001");
      assert.equal(
        await page
          .getByRole("button", { name: "Change photo location" })
          .count(),
        0,
      );
      assert.equal(directGeocoderRequests, 0);
      await name.fill("Forms Tester");
      await description.fill("Painted shop lettering");
      policy["/api/v1/letterings/upload"] = "hold";
      await page.locator("form").evaluate((node) => {
        node.requestSubmit();
        node.requestSubmit();
      });
      await until(() => held.length === 1);
      assert.equal(count("letterings/upload"), 1);
      assert.equal(await name.isDisabled(), true);
      await held
        .shift()
        .route.fulfill(fail("Image storage is temporarily unavailable"));
      await page
        .getByRole("alert")
        .filter({
          hasText: "lettering.png: Image storage is temporarily unavailable",
        })
        .waitFor();
      assert.equal(await name.inputValue(), "Forms Tester");
      assert.equal(await description.inputValue(), "Painted shop lettering");
      assert.equal(await remove.count(), 1);
      await assertFits();
      await page.screenshot({
        path: `/tmp/ttl-upload-form-${width}.png`,
        fullPage: true,
      });
      policy["/api/v1/letterings/upload"] = {
        json: { id: "created", location_source: "postal_code" },
      };
      await page
        .getByRole("button", { name: "Upload photo", exact: true })
        .click();
      await page.waitForURL(`${origin}/`);
      assert.equal(count("letterings/upload"), 2);
      assert.equal(
        requests.filter((r) => r.path.endsWith("/letterings/upload"))[0].key,
        requests.filter((r) => r.path.endsWith("/letterings/upload"))[1].key,
      );
      assert.deepEqual(browserErrors, []);
      await context.close();
      console.log(
        `Auth/reset/upload keyboard, retry, cancellation and layout checks passed at ${width}px`,
      );
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
