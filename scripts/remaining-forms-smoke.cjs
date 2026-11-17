const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium } = require("playwright");
const root = path.resolve(__dirname, "..");
const date = "2026-11-17T00:00:00Z";
const user = {
  id: "forms-owner",
  email: "owner@example.test",
  display_name: "Form Tester",
  role: "USER",
  created_at: date,
};
const photo = {
  id: "photo",
  image_url:
    "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==",
  thumbnail_small: "",
  contributor_tag: "Form Tester",
  pin_code: "10001",
  description: "Painted shop sign",
  detected_text: "A shop sign",
  status: "PENDING",
  likes_count: 0,
  comments_count: 0,
  report_count: 0,
  moderation_reason: null,
  moderated_at: null,
  moderated_by: null,
  created_at: date,
  updated_at: date,
};
const city = {
  id: "city",
  name: "AnUnbrokenCityName".repeat(5),
  country_code: "US",
  center_lat: 40.7,
  center_lng: -74,
  default_zoom: 10,
  is_active: true,
  description: "City description",
};
const policyRow = {
  country_code: "US",
  uploads_enabled: true,
  comments_enabled: true,
  discoverability_enabled: true,
  auto_moderation_level: "standard",
  created_at: date,
  updated_at: date,
};
const record = {
  id: "photo",
  city_name: "New York",
  country_code: "US",
  pin_code: "10001",
  image_url: photo.image_url,
  lat: null,
  lng: null,
  location_source: "unresolved",
  location_error: "provider_unavailable",
  updated_at: date,
};
const until = async (condition) => {
  const deadline = Date.now() + 10000;
  while (!condition() && Date.now() < deadline)
    await new Promise((resolve) => setTimeout(resolve, 20));
  assert.ok(condition(), "Expected intercepted request was not received");
};
const failed = (message, status = 503) => ({
  status,
  json: { error: message },
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
        sessionStorage.setItem("ttl_user_token", "owner-token");
        sessionStorage.setItem(
          "ttl_user_account",
          JSON.stringify({ token: "owner-token", user: account }),
        );
      }, user);
      await context.route("https://fonts.googleapis.com/**", (route) =>
        route.abort(),
      );
      await context.routeWebSocket("**/ws/feed", () => {});
      const requests = [],
        held = [],
        policy = {};
      const collections = [];
      let repaired = false;
      let savedPhoto = { ...photo },
        savedPolicy = { ...policyRow };
      const payload = (route) => route.request().postDataJSON();
      const key = (method, pathname) => `${method} ${pathname}`;
      await context.route("**/api/v1/**", (route) => {
        const request = route.request(),
          url = new URL(request.url());
        const endpoint = key(request.method(), url.pathname);
        requests.push({
          endpoint,
          query: Object.fromEntries(url.searchParams),
          body: request.postData() ? request.postDataJSON() : null,
        });
        if (policy[endpoint] === "hold") {
          held.push({ endpoint, route });
          return;
        }
        if (policy[endpoint]) return route.fulfill(policy[endpoint]);
        if (url.pathname === "/api/v1/auth/me")
          return route.fulfill({ json: user });
        if (url.pathname === "/api/v1/admin/login")
          return route.fulfill({ json: { token: "admin-token" } });
        if (
          url.pathname === "/api/v1/me/letterings/photo" &&
          request.method() === "PATCH"
        ) {
          savedPhoto = { ...savedPhoto, ...payload(route) };
          return route.fulfill({ json: savedPhoto });
        }
        if (url.pathname === "/api/v1/me/letterings")
          return route.fulfill({
            json: { items: [savedPhoto], total: 1, limit: 50, offset: 0 },
          });
        if (url.pathname.startsWith("/api/v1/contributors/"))
          return route.fulfill({
            json: {
              contributor_tag: decodeURIComponent(
                url.pathname.split("/").at(-1),
              ),
              total_count: 0,
              letterings: [],
            },
          });
        if (url.pathname === "/api/v1/community/leaderboard")
          return route.fulfill({
            json: [{ tag: "Form Tester", count: 1, total_likes: 1 }],
          });
        if (url.pathname === "/api/v1/collections") {
          if (request.method() === "POST") {
            collections.push({
              id: "collection",
              name: payload(route).name,
              description: payload(route).description,
              creator_tag: user.display_name,
              item_count: 0,
              is_public: true,
              created_at: date,
            });
            return route.fulfill({
              status: 201,
              json: {
                id: "collection",
                name: payload(route).name,
                creator_tag: user.display_name,
              },
            });
          }
          return route.fulfill({
            json: {
              items: collections,
              total: collections.length,
              limit: 20,
              offset: 0,
            },
          });
        }
        if (url.pathname === "/api/v1/cities")
          return route.fulfill({ json: [city] });
        if (url.pathname === "/api/v1/admin/stats")
          return route.fulfill({
            json: {
              total_uploads: 1,
              pending_approvals: 1,
              approved: 0,
              rejected: 0,
              total_cities: 1,
              total_likes: 0,
              total_comments: 0,
            },
          });
        if (url.pathname === "/api/v1/admin/region-policies/US") {
          savedPolicy = { ...savedPolicy, ...payload(route) };
          return route.fulfill({ json: savedPolicy });
        }
        if (url.pathname === "/api/v1/admin/region-policies")
          return route.fulfill({
            json: { items: [savedPolicy], total: 1, limit: 500, offset: 0 },
          });
        if (url.pathname === "/api/v1/admin/audit-logs")
          return route.fulfill({
            json: { items: [], total: 0, limit: 10, offset: 0 },
          });
        if (url.pathname === "/api/v1/admin/location-repairs")
          return route.fulfill({
            json: { items: repaired ? [] : [record], next_cursor: null },
          });
        if (url.pathname.endsWith("/history"))
          return route.fulfill({ json: { items: [], next_cursor: null } });
        if (url.pathname.endsWith("/preview"))
          return route.fulfill({
            json: {
              repair_id: "repair",
              current: record,
              evidence: payload(route),
            },
          });
        if (url.pathname.endsWith("/apply")) {
          repaired = true;
          return route.fulfill({
            json: {
              id: "repair",
              lettering_id: "photo",
              reviewed_by: "admin",
              before_location: record,
              after_location: {
                ...record,
                ...payload(route).evidence,
                location_source: "postal_code",
              },
              evidence: payload(route).evidence,
              created_at: date,
            },
          });
        }
        if (url.pathname.includes("/admin/jobs/"))
          return route.fulfill({ json: { items: [], next_cursor: null } });
        return route.fulfill({
          json: { items: [], letterings: [], total: 0, limit: 25, offset: 0 },
        });
      });
      const page = await context.newPage();
      page.setDefaultTimeout(12000);
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      const count = (endpoint) =>
        requests.filter((r) => r.endpoint === endpoint).length;
      const fits = async () =>
        assert.ok(
          await page.evaluate(
            () =>
              document.documentElement.scrollWidth <=
              document.documentElement.clientWidth,
          ),
          `Horizontal overflow at ${width}px on ${page.url()}`,
        );
      const focused = async (locator) =>
        assert.equal(
          await locator.evaluate((node) => node === document.activeElement),
          true,
        );
      const contrast = async (locator) => {
        const ratio = await locator.evaluate((node) => {
          const rgb = (value) => value.match(/[\d.]+/g).map(Number);
          const luminance = (values) =>
            values
              .slice(0, 3)
              .map((v) => {
                v /= 255;
                return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
              })
              .reduce((sum, v, i) => sum + v * [0.2126, 0.7152, 0.0722][i], 0);
          let background = node;
          while (
            background.parentElement &&
            getComputedStyle(background).backgroundColor === "rgba(0, 0, 0, 0)"
          )
            background = background.parentElement;
          const a = luminance(rgb(getComputedStyle(node).color)),
            b = luminance(rgb(getComputedStyle(background).backgroundColor));
          return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
        });
        assert.ok(
          ratio >= 4.5,
          `Text contrast ${ratio.toFixed(2)} is below 4.5`,
        );
      };
      const fieldContrast = async (locator) => {
        const ratios = await locator.evaluate((node) => {
          node.blur();
          const rgba = (value) => value.match(/[\d.]+/g).map(Number);
          const luminance = (values) =>
            values
              .slice(0, 3)
              .map((v) => {
                v /= 255;
                return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
              })
              .reduce((sum, v, i) => sum + v * [0.2126, 0.7152, 0.0722][i], 0);
          const ratio = (a, b) =>
            (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
          const style = getComputedStyle(node);
          const background = rgba(style.backgroundColor);
          const border = rgba(style.borderTopColor);
          const painted = border
            .slice(0, 3)
            .map(
              (v, i) =>
                v * (border[3] ?? 1) + background[i] * (1 - (border[3] ?? 1)),
            );
          return {
            border: ratio(luminance(painted), luminance(background)),
            placeholder: ratio(
              luminance(rgba(getComputedStyle(node, "::placeholder").color)),
              luminance(background),
            ),
          };
        });
        assert.ok(
          ratios.border >= 3,
          `Input border contrast ${ratios.border.toFixed(2)} is below 3`,
        );
        assert.ok(
          ratios.placeholder >= 4.5,
          `Placeholder contrast ${ratios.placeholder.toFixed(2)} is below 4.5`,
        );
      };
      console.log(`Checking remaining forms at ${width}px`);
      const profilePath = "/api/v1/contributors/Form%20Tester";
      policy[key("GET", profilePath)] = failed("Profile service unavailable");
      await page.goto(`${origin}/community`);
      const contributor = page.getByRole("button", {
        name: "Form Tester",
        exact: true,
      });
      await contributor.focus();
      await page.keyboard.press("Enter");
      await page.waitForURL(`${origin}/contributor/Form%20Tester`);
      await page
        .getByRole("alert")
        .filter({ hasText: "Contributions could not be loaded" })
        .waitFor();
      assert.equal(
        await page
          .getByText("No approved contributions yet", { exact: true })
          .count(),
        0,
      );
      assert.equal(
        await page
          .getByText("0 contributions archived", { exact: true })
          .count(),
        0,
      );
      delete policy[key("GET", profilePath)];
      await page
        .getByRole("button", { name: "Try again", exact: true })
        .click();
      const empty = page.getByText("No approved contributions yet", {
        exact: true,
      });
      await empty.waitFor();
      await contrast(empty);
      await fits();
      await page
        .getByRole("button", {
          name: "Back from contributor profile",
          exact: true,
        })
        .click();
      await page.waitForURL(`${origin}/community`);
      await page
        .getByRole("button", { name: "Collections", exact: true })
        .click();
      const newCollection = page.getByRole("button", {
        name: "New Collection",
        exact: true,
      });
      await newCollection.click();
      const dialog = page.getByRole("dialog", {
        name: "New collection",
        exact: true,
      });
      const collectionName = dialog.getByLabel("Collection name", {
        exact: true,
      });
      await focused(collectionName);
      await collectionName.fill("   ");
      await dialog.getByRole("button", { name: "Create", exact: true }).click();
      await dialog.getByRole("alert").waitFor();
      await focused(collectionName);
      assert.equal(await collectionName.getAttribute("aria-invalid"), "true");
      assert.equal(
        await collectionName.evaluate(
          (node) =>
            document.getElementById(node.getAttribute("aria-describedby"))
              ?.textContent,
        ),
        "Enter a collection name.",
      );
      assert.equal(count("POST /api/v1/collections"), 0);
      await collectionName.fill("Field notes");
      assert.equal(await collectionName.getAttribute("aria-invalid"), "false");
      await dialog
        .getByLabel("Description (optional)", { exact: true })
        .fill("Keep this description after a failure");
      policy["POST /api/v1/collections"] = "hold";
      await dialog.locator("form").evaluate((form) => {
        form.requestSubmit();
        form.requestSubmit();
      });
      await until(() => held.length === 1);
      assert.equal(count("POST /api/v1/collections"), 1);
      await held
        .shift()
        .route.fulfill(failed("Collections are temporarily unavailable"));
      await dialog
        .getByRole("alert")
        .filter({ hasText: "Collections are temporarily unavailable" })
        .waitFor();
      assert.equal(await collectionName.inputValue(), "Field notes");
      await contrast(dialog.getByRole("alert"));
      await fits();
      delete policy["POST /api/v1/collections"];
      policy["GET /api/v1/collections"] = failed("Collection list unavailable");
      await dialog.getByRole("button", { name: "Create", exact: true }).click();
      await dialog.waitFor({ state: "detached" });
      await focused(newCollection);
      await page
        .getByRole("alert")
        .filter({ hasText: "Community data could not be loaded" })
        .waitFor();
      assert.equal(count("POST /api/v1/collections"), 2);
      assert.equal(
        await page.getByText("Collection created", { exact: true }).count(),
        1,
      );
      delete policy["GET /api/v1/collections"];
      await page
        .getByRole("button", { name: "Try again", exact: true })
        .click();
      await page
        .getByRole("link", { name: "Field notes", exact: true })
        .waitFor();
      await newCollection.click();
      await page.keyboard.press("Escape");
      await dialog.waitFor({ state: "detached" });
      await focused(newCollection);
      await page.evaluate(() => {
        const original = window.fetch;
        window.fetch = (url, init) =>
          original(url, { ...init, signal: undefined });
      });
      await newCollection.click();
      await dialog
        .getByLabel("Collection name", { exact: true })
        .fill("Cancelled collection");
      policy["POST /api/v1/collections"] = "hold";
      await dialog.getByRole("button", { name: "Create", exact: true }).click();
      await until(() => held.length === 1);
      await page.keyboard.press("Escape");
      await dialog.waitFor({ state: "detached" });
      await focused(newCollection);
      const cancelledResponse = page.waitForResponse(
        (response) =>
          response.url().endsWith("/collections") && response.status() === 401,
      );
      await held
        .shift()
        .route.fulfill(failed("Cancelled request rejected", 401));
      await cancelledResponse;
      await page.waitForLoadState("networkidle");
      assert.equal(
        await page.evaluate(() => sessionStorage.getItem("ttl_user_token")),
        "owner-token",
        "A cancelled request must not expire the current session",
      );
      delete policy["POST /api/v1/collections"];
      console.log(`Profile and collection recovery passed at ${width}px`);

      await page.goto(`${origin}/me/uploads`);
      const edit = page.getByRole("button", { name: "Edit", exact: true });
      await edit.click();
      const editForm = page.getByRole("form", {
        name: "Edit upload",
        exact: true,
      });
      const tag = editForm.getByLabel("Contributor tag", { exact: true });
      const description = editForm.getByLabel("Description (optional)", {
        exact: true,
      });
      await focused(tag);
      await fieldContrast(tag);
      await tag.fill("!");
      await tag.evaluate((node) =>
        node.addEventListener(
          "focus",
          () => {
            window.editErrorAtFocus = {
              invalid: node.getAttribute("aria-invalid"),
              message: document.getElementById(
                node.getAttribute("aria-describedby"),
              )?.textContent,
            };
          },
          { once: true },
        ),
      );
      await editForm.getByRole("button", { name: "Save", exact: true }).click();
      assert.equal(await tag.getAttribute("aria-invalid"), "true");
      assert.deepEqual(await page.evaluate(() => window.editErrorAtFocus), {
        invalid: "true",
        message: "Contributor tag must be 2 to 30 characters.",
      });
      await focused(tag);
      assert.equal(count("PATCH /api/v1/me/letterings/photo"), 0);
      await tag.fill("Edited Tag");
      await description.fill("Retain this edit after a failed save");
      policy["PATCH /api/v1/me/letterings/photo"] = "hold";
      await editForm.evaluate((form) => {
        form.requestSubmit();
        form.requestSubmit();
      });
      await until(() => held.length === 1);
      assert.equal(count("PATCH /api/v1/me/letterings/photo"), 1);
      await held.shift().route.fulfill(failed("Metadata service unavailable"));
      await editForm.getByRole("alert").waitFor();
      await focused(editForm.getByRole("alert"));
      assert.equal(
        await description.inputValue(),
        "Retain this edit after a failed save",
      );
      await fits();
      await page.screenshot({
        path: `/tmp/ttl-upload-edit-${width}.png`,
        fullPage: true,
      });
      delete policy["PATCH /api/v1/me/letterings/photo"];
      await tag.focus();
      await page.keyboard.press("Enter");
      await editForm.waitFor({ state: "detached" });
      await focused(edit);
      await page
        .getByText("Retain this edit after a failed save", { exact: true })
        .waitFor();
      await edit.click();
      await editForm
        .getByRole("button", { name: "Cancel", exact: true })
        .click();
      await focused(edit);
      console.log(
        `Upload edit validation, retry and focus passed at ${width}px`,
      );
      await edit.click();
      const allUploads = page.getByRole("button", { name: "ALL", exact: true });
      await allUploads.click();
      await editForm.waitFor({ state: "detached" });
      await focused(allUploads);

      await page.goto(`${origin}/admin`);
      const signIn = page.getByRole("form", {
        name: "Admin sign in",
        exact: true,
      });
      await signIn
        .getByLabel("Email", { exact: true })
        .fill("admin@example.test");
      await signIn
        .getByLabel("Password", { exact: true })
        .fill("fixture-password");
      policy["POST /api/v1/admin/login"] = failed("Admin sign-in unavailable");
      await signIn
        .getByRole("button", { name: "Sign in", exact: true })
        .click();
      await signIn.getByRole("alert").waitFor();
      await focused(signIn.getByRole("alert"));
      assert.equal(
        await signIn.getByLabel("Email", { exact: true }).inputValue(),
        "admin@example.test",
      );
      delete policy["POST /api/v1/admin/login"];
      await signIn
        .getByRole("button", { name: "Sign in", exact: true })
        .click();
      const nav = page.getByRole("navigation", {
        name: "Admin sections",
        exact: true,
      });
      await nav.waitFor();
      for (const tab of [
        "Moderation",
        "Flags",
        "Comments",
        "Failed jobs",
        "Activity",
      ]) {
        await nav.getByRole("button", { name: tab, exact: true }).click();
        await page.waitForLoadState("networkidle");
        await fits();
      }
      policy["GET /api/v1/cities"] = failed("Cities are unavailable");
      await nav.getByRole("button", { name: "Cities", exact: true }).click();
      await page
        .getByRole("button", { name: "Retry cities", exact: true })
        .waitFor();
      assert.equal(
        await page
          .getByText("No cities found for current filter.", { exact: true })
          .count(),
        0,
      );
      delete policy["GET /api/v1/cities"];
      await page
        .getByRole("button", { name: "Retry cities", exact: true })
        .click();
      await page
        .getByRole("heading", { name: city.name, exact: true })
        .waitFor();
      const cityForm = page.getByRole("form", {
        name: "Global City Operations",
        exact: true,
      });
      const query = cityForm.getByLabel("Search city", { exact: true });
      const country = cityForm.getByLabel("Country code (optional)", {
        exact: true,
      });
      const limit = cityForm.getByLabel("Result limit", { exact: true });
      await fieldContrast(query);
      await limit.fill("0");
      const cityReads = count("GET /api/v1/cities");
      await cityForm.getByRole("button", { name: "Load", exact: true }).click();
      assert.equal(count("GET /api/v1/cities"), cityReads);
      assert.equal(
        await limit.evaluate((node) => node.validity.rangeUnderflow),
        true,
      );
      await limit.fill("50");
      await country.fill("US");
      await query.fill("New York");
      policy["POST /api/v1/admin/cities/discover"] = "hold";
      const discover = cityForm.getByRole("button", {
        name: "Find cities",
        exact: true,
      });
      await discover.focus();
      await discover.evaluate((button) => {
        button.click();
        button.click();
      });
      await until(() => held.length === 1);
      assert.equal(count("POST /api/v1/admin/cities/discover"), 1);
      await held.shift().route.fulfill(failed("City discovery unavailable"));
      await cityForm.getByRole("alert").waitFor();
      await focused(discover);
      assert.equal(await query.inputValue(), "New York");
      policy["POST /api/v1/admin/cities/discover"] = {
        json: { processed: 2, upserted: 1, failed: 1 },
      };
      await discover.click();
      await cityForm
        .getByRole("status")
        .filter({ hasText: "Failed 1" })
        .waitFor();
      policy["POST /api/v1/admin/cities/bootstrap-capitals"] = failed(
        "Capital city service unavailable",
      );
      page.once("dialog", (dialog) => dialog.accept());
      const capitals = cityForm.getByRole("button", {
        name: "Add capital cities",
        exact: true,
      });
      await capitals.click();
      await cityForm
        .getByRole("alert")
        .filter({ hasText: "Capital city service unavailable" })
        .waitFor();
      await focused(capitals);
      await fits();
      await page.screenshot({
        path: `/tmp/ttl-admin-cities-${width}.png`,
        fullPage: true,
      });

      policy["GET /api/v1/admin/region-policies"] = failed(
        "Policies unavailable",
      );
      policy["GET /api/v1/admin/audit-logs"] = failed(
        "Policy history unavailable",
      );
      await nav.getByRole("button", { name: "Regions", exact: true }).click();
      await page
        .getByRole("button", { name: "Retry policies", exact: true })
        .waitFor();
      await page
        .getByRole("button", { name: "Retry history", exact: true })
        .waitFor();
      assert.equal(
        await page
          .getByText("No region policies found.", { exact: true })
          .count(),
        0,
      );
      assert.equal(
        await page
          .getByText("No policy audit events found.", { exact: true })
          .count(),
        0,
      );
      delete policy["GET /api/v1/admin/region-policies"];
      delete policy["GET /api/v1/admin/audit-logs"];
      await page
        .getByRole("button", { name: "Retry policies", exact: true })
        .click();
      await page
        .getByRole("button", { name: "Retry history", exact: true })
        .click();
      const region = page.getByRole("form", {
        name: "Policy for US",
        exact: true,
      });
      await region.getByLabel("Uploads", { exact: true }).uncheck();
      policy["PUT /api/v1/admin/region-policies/US"] = "hold";
      await region.evaluate((form) => {
        form.requestSubmit();
        form.requestSubmit();
      });
      await until(() => held.length === 1);
      assert.equal(count("PUT /api/v1/admin/region-policies/US"), 1);
      await held.shift().route.fulfill(failed("Policy update unavailable"));
      await page
        .getByRole("alert")
        .filter({ hasText: "Policy update unavailable" })
        .waitFor();
      assert.equal(
        await region.getByLabel("Uploads", { exact: true }).isChecked(),
        false,
      );
      delete policy["PUT /api/v1/admin/region-policies/US"];
      await region.getByRole("button", { name: "Save", exact: true }).click();
      await page
        .getByRole("status")
        .filter({ hasText: "Policy updated for US." })
        .waitFor();
      const regionFilter = page.getByRole("form", {
        name: "Region Policy Controls",
        exact: true,
      });
      await regionFilter
        .getByRole("button", { name: "Apply default policy", exact: true })
        .click();
      await regionFilter.getByRole("alert").waitFor();
      const countryFilter = regionFilter.getByLabel(
        "Country code (optional filter)",
        { exact: true },
      );
      await focused(countryFilter);
      await countryFilter.fill("US");
      page.once("dialog", (dialog) => dialog.dismiss());
      await regionFilter
        .getByRole("button", { name: "Apply default policy", exact: true })
        .click();
      assert.equal(count("PUT /api/v1/admin/region-policies/US"), 2);
      assert.equal(savedPolicy.uploads_enabled, false);
      page.once("dialog", (dialog) => dialog.accept());
      await regionFilter
        .getByRole("button", { name: "Apply default policy", exact: true })
        .click();
      await page
        .getByRole("status")
        .filter({ hasText: "Default settings applied for US." })
        .waitFor();
      await page.waitForLoadState("networkidle");
      assert.equal(savedPolicy.uploads_enabled, true);
      assert.equal(
        await region.getByLabel("Uploads", { exact: true }).isChecked(),
        true,
      );

      await fits();
      await page.screenshot({
        path: `/tmp/ttl-admin-regions-${width}.png`,
        fullPage: true,
      });
      console.log(`Admin login, city and region recovery passed at ${width}px`);

      policy["GET /api/v1/admin/location-repairs"] = failed(
        "Location report unavailable",
      );
      await nav.getByRole("button", { name: "Locations", exact: true }).click();
      await page
        .getByRole("button", { name: "Retry report", exact: true })
        .waitFor();
      delete policy["GET /api/v1/admin/location-repairs"];
      await page
        .getByRole("button", { name: "Retry report", exact: true })
        .click();
      const review = page.getByRole("button", {
        name: "Review location",
        exact: true,
      });
      await review.waitFor();
      policy["GET /api/v1/admin/location-repairs"] = "hold";
      await page
        .getByRole("button", { name: "Refresh report", exact: true })
        .click();
      await until(() => held.length === 1);
      await review.click();
      await page
        .getByRole("button", { name: "Back to report", exact: true })
        .click();
      await focused(review);
      const showResolved = page.getByRole("checkbox", {
        name: "Show resolved uploads",
        exact: true,
      });
      await showResolved.focus();
      await held
        .shift()
        .route.fulfill({ json: { items: [record], next_cursor: null } });
      delete policy["GET /api/v1/admin/location-repairs"];
      await page.waitForLoadState("networkidle");
      await focused(showResolved);
      await review.click();
      await focused(
        page.getByRole("heading", { name: "New York · 10001", exact: true }),
      );
      const evidenceForm = page.getByRole("form", {
        name: "Location correction evidence",
        exact: true,
      });
      await evidenceForm
        .getByRole("button", { name: "Preview correction", exact: true })
        .click();
      assert.equal(
        count("POST /api/v1/admin/location-repairs/photo/preview"),
        0,
      );
      const fields = {
        "Source city": "New York",
        "Source country code": "US",
        "Source postal code": "10001",
        Latitude: "40.75",
        Longitude: "-73.99",
        "Source name": "Postal source",
        "Source URL": "https://example.test/postal",
        "Reason for correction": "Checked postal district",
      };
      for (const [label, value] of Object.entries(fields))
        await evidenceForm.getByLabel(label, { exact: true }).fill(value);
      policy["POST /api/v1/admin/location-repairs/photo/preview"] = failed(
        "Preview unavailable",
      );
      await evidenceForm
        .getByRole("button", { name: "Preview correction", exact: true })
        .click();
      await page
        .getByRole("alert")
        .filter({ hasText: "Preview unavailable" })
        .waitFor();
      await focused(
        page.getByRole("alert").filter({ hasText: "Preview unavailable" }),
      );
      assert.equal(
        await evidenceForm
          .getByLabel("Source city", { exact: true })
          .inputValue(),
        "New York",
      );
      delete policy["POST /api/v1/admin/location-repairs/photo/preview"];
      await evidenceForm
        .getByRole("button", { name: "Preview correction", exact: true })
        .click();
      const preview = page.getByRole("region", {
        name: "Correction preview",
        exact: true,
      });
      await preview.waitFor();
      await page.waitForFunction(
        () =>
          document.activeElement?.getAttribute("aria-label") ===
          "Correction preview",
      );
      const approve = preview.getByRole("button", {
        name: "Approve correction",
        exact: true,
      });
      assert.equal(await approve.isDisabled(), true);
      await preview.getByRole("checkbox").check();
      policy["POST /api/v1/admin/location-repairs/photo/apply"] = "hold";
      await approve.evaluate((button) => {
        button.click();
        button.click();
      });
      await until(() => held.length === 1);
      assert.equal(count("POST /api/v1/admin/location-repairs/photo/apply"), 1);
      await held.shift().route.fulfill(failed("Correction could not be saved"));
      await preview.getByRole("alert").waitFor();
      await focused(preview.getByRole("alert"));
      assert.equal(
        await evidenceForm
          .getByLabel("Reason for correction", { exact: true })
          .inputValue(),
        "Checked postal district",
      );
      await fits();
      await page.screenshot({
        path: `/tmp/ttl-admin-location-${width}.png`,
        fullPage: true,
      });
      await page
        .getByRole("button", { name: "Back to report", exact: true })
        .click();
      await focused(review);
      await review.click();
      for (const [label, value] of Object.entries(fields))
        await evidenceForm.getByLabel(label, { exact: true }).fill(value);
      await evidenceForm
        .getByRole("button", { name: "Preview correction", exact: true })
        .click();
      await preview.getByRole("checkbox").check();
      delete policy["POST /api/v1/admin/location-repairs/photo/apply"];
      await approve.click();
      await page
        .getByRole("status")
        .filter({ hasText: "Correction saved for New York 10001." })
        .waitFor();
      await page.waitForLoadState("networkidle");
      await focused(
        page.getByRole("heading", { name: "Location repairs", exact: true }),
      );
      assert.equal(count("POST /api/v1/admin/location-repairs/photo/apply"), 2);
      const correction = requests
        .filter(
          (request) =>
            request.endpoint ===
            "POST /api/v1/admin/location-repairs/photo/apply",
        )
        .at(-1).body;
      assert.equal(correction.reviewed, true);
      assert.equal(correction.expected_updated_at, date);
      assert.equal(correction.evidence.lat, 40.75);
      assert.equal(correction.evidence.lng, -73.99);
      policy["GET /api/v1/admin/location-repairs"] = failed(
        "Admin session expired",
        401,
      );
      await page
        .getByRole("button", { name: "Refresh report", exact: true })
        .click();
      await signIn.waitFor();
      await signIn
        .getByRole("alert")
        .filter({ hasText: "Admin session expired" })
        .waitFor();
      assert.equal(
        await page.evaluate(() => sessionStorage.getItem("ttl_admin_token")),
        null,
      );
      assert.deepEqual(errors, []);
      await context.close();
      console.log(
        `Location correction validation, retries, focus and expiry passed at ${width}px`,
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
