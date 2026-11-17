const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium } = require("playwright");
const root = path.resolve(__dirname, "..");
const image = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a/S8AAAAASUVORK5CYII=",
  "base64",
);
const user = {
  id: "pin-owner",
  email: "pin@example.test",
  role: "USER",
  created_at: "2026-11-17",
};
const original = {
  id: "pin-photo",
  contributor_tag: "आशा",
  pin_code: "10001",
  description: "Legacy attribution",
  status: "APPROVED",
  latitude: 40.7,
  longitude: -74,
  location_source: "contributor_pin",
  location_name: "Old place",
  location_accuracy_m: null,
  image_url: "",
  thumbnail_small: "",
  created_at: "2026-11-17",
  updated_at: "2026-11-17",
};
const place = {
  name: "A very long place name ".repeat(8),
  latitude: 51.5,
  longitude: -0.1,
};
const until = async (condition) => {
  const end = Date.now() + 10000;
  while (!condition() && Date.now() < end)
    await new Promise((resolve) => setTimeout(resolve, 20));
  assert.ok(condition());
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
    browser = await chromium.launch({ channel: "chromium", headless: true });
    for (const width of [1280, 320]) {
      const context = await browser.newContext({
        viewport: { width, height: 800 },
        serviceWorkers: "block",
      });
      await context.addInitScript((account) => {
        sessionStorage.setItem("ttl_user_token", "pin-token");
        sessionStorage.setItem(
          "ttl_user_account",
          JSON.stringify({ token: "pin-token", user: account }),
        );
      }, user);
      await context.route("https://fonts.googleapis.com/**", (route) =>
        route.abort(),
      );
      await context.route("https://*.tile.openstreetmap.org/**", (route) =>
        route.abort(),
      );
      await context.routeWebSocket("**/ws/feed", () => {});
      const searches = [],
        uploads = [],
        edits = [],
        errors = [];
      let searchPolicy = "ok",
        uploadPolicy = "fail",
        pendingSearch;
      let saved = { ...original };
      await context.route("**/api/v1/**", async (route) => {
        const req = route.request(),
          url = new URL(req.url());
        if (url.pathname === "/api/v1/auth/me")
          return route.fulfill({ json: user });
        if (url.pathname === "/api/v1/places") {
          searches.push(url.searchParams.get("q"));
          if (searchPolicy === "hold") {
            pendingSearch = route;
            return;
          }
          if (searchPolicy === "fail")
            return route.fulfill({
              status: 503,
              json: { error: "Place search unavailable" },
            });
          return route.fulfill({ json: [place] });
        }
        if (url.pathname === "/api/v1/letterings/upload") {
          const form = await new Response(req.postDataBuffer(), {
            headers: { "Content-Type": req.headers()["content-type"] },
          }).formData();
          uploads.push({
            location: form.get("location") && JSON.parse(form.get("location")),
            key: req.headers()["idempotency-key"],
            postal: form.get("pin_code"),
          });
          return route.fulfill(
            uploadPolicy === "fail"
              ? { status: 503, json: { error: "Retry this upload" } }
              : {
                  json: { id: "new-photo", location_source: "contributor_pin" },
                },
          );
        }
        if (
          url.pathname === "/api/v1/me/letterings/pin-photo" &&
          req.method() === "PATCH"
        ) {
          const payload = req.postDataJSON();
          edits.push(payload);
          if (payload.location?.action === "set")
            saved = {
              ...saved,
              ...payload.location.point,
              location_name: payload.location.point.place_name,
              location_accuracy_m: payload.location.point.accuracy_m,
            };
          if (payload.location?.action === "clear")
            saved = {
              ...saved,
              latitude: null,
              longitude: null,
              location_name: null,
              location_source: "unresolved",
            };
          return route.fulfill({ json: saved });
        }
        if (url.pathname === "/api/v1/me/letterings")
          return route.fulfill({
            json: { items: [saved], total: 1, limit: 50, offset: 0 },
          });
        if (url.pathname.endsWith("/timeline"))
          return route.fulfill({
            json: {
              status_history: [],
              metadata_history: [
                {
                  id: "history",
                  field_name: "location",
                  old_value: JSON.stringify({
                    latitude: 40.7,
                    longitude: -74,
                    place_name: "Old place",
                    accuracy_m: null,
                  }),
                  new_value: JSON.stringify({
                    latitude: null,
                    longitude: null,
                  }),
                  created_at: "2026-11-17",
                },
              ],
            },
          });
        return route.fulfill({
          json:
            url.pathname === "/api/v1/cities" ? [] : { items: [], total: 0 },
        });
      });
      const page = await context.newPage();
      page.setDefaultTimeout(10000);
      page.on("pageerror", (error) => errors.push(error.message));
      const picker = () =>
        page.getByRole("dialog", { name: "Choose photo location" });
      const fits = async () => {
        assert.ok(
          await page.evaluate(
            () =>
              document.documentElement.scrollWidth <=
              document.documentElement.clientWidth,
          ),
        );
        assert.ok(
          await picker().evaluate(
            (node) => node.scrollWidth <= node.clientWidth,
          ),
          "Picker must fit narrow screens",
        );
      };
      await page.goto(`${origin}/contribute`);
      await page.evaluate(async () => {
        const { useCityStore } = await import("/src/store/useCityStore.ts");
        useCityStore.getState().setCity("city", "London", 51.5, -0.1, 12);
      });
      const file = page.waitForEvent("filechooser");
      await page
        .getByRole("button", { name: "Choose photos", exact: true })
        .click();
      await (
        await file
      ).setFiles({ name: "pin.png", mimeType: "image/png", buffer: image });
      await page
        .getByLabel("Contributor name", { exact: true })
        .fill("Pin Tester");
      await page
        .getByRole("textbox", { name: "Postal or ZIP code", exact: true })
        .fill("10001");
      const openPicker = () =>
        page
          .getByRole("button", { name: /^(Choose|Change) photo location$/ })
          .click();
      await openPicker();
      await picker()
        .getByLabel("Search for a place", { exact: true })
        .fill("London");
      assert.equal(
        searches.length,
        0,
        "Typing must not send geocoder requests",
      );
      searchPolicy = "fail";
      await picker()
        .getByRole("button", { name: "Search place", exact: true })
        .click();
      await picker().getByRole("alert").waitFor();
      assert.equal(
        await picker()
          .getByRole("alert")
          .evaluate((node) => node === document.activeElement),
        true,
      );
      assert.equal(uploads.length, 0, "Search must not submit the upload form");
      searchPolicy = "hold";
      await picker()
        .getByRole("button", { name: "Search place", exact: true })
        .click();
      await until(() => pendingSearch);
      await picker()
        .getByLabel("Search for a place", { exact: true })
        .fill("Paris");
      await pendingSearch
        .fulfill({ json: [{ ...place, name: "Stale result" }] })
        .catch(() => {});
      searchPolicy = "ok";
      await picker()
        .getByRole("button", { name: "Search place", exact: true })
        .click();
      await picker()
        .getByRole("button", { name: place.name.trim(), exact: true })
        .click();
      assert.equal(
        await picker()
          .getByRole("button", { name: "Stale result", exact: true })
          .count(),
        0,
      );
      await picker()
        .getByLabel("Estimated accuracy", { exact: true })
        .selectOption("100");
      await fits();
      assert.equal(uploads.length, 0);
      await picker()
        .getByRole("button", { name: "Confirm location", exact: true })
        .click();
      assert.equal(uploads.length, 0, "Confirmation must not submit photos");
      await page
        .getByRole("button", { name: "Upload photo", exact: true })
        .click();
      await until(() => uploads.length === 1);
      await page
        .getByText("Retry this upload", { exact: true })
        .first()
        .waitFor();
      const selected = {
        latitude: place.latitude,
        longitude: place.longitude,
        place_name: place.name.trim(),
        accuracy_m: 100,
      };
      assert.deepEqual(uploads[0].location, selected);
      // Editing a point must change the retry key; cancelling must preserve it.
      await openPicker();
      await picker()
        .getByLabel("Place name", { exact: true })
        .fill("Discarded change");
      await page.keyboard.press("Escape");
      assert.equal(
        await page
          .getByRole("button", { name: "Change photo location" })
          .evaluate((node) => node === document.activeElement),
        true,
      );
      await page
        .getByRole("button", { name: "Upload photo", exact: true })
        .click();
      await until(() => uploads.length === 2);
      assert.equal(uploads[1].key, uploads[0].key);
      await openPicker();
      const map = picker().locator(".leaflet-container");
      await map.focus();
      await map.evaluate((node) => {
        window.pickerTransform =
          node.querySelector(".leaflet-map-pane").style.transform;
      });
      await page.keyboard.press("ArrowRight");
      await page.waitForFunction(
        () =>
          document.querySelector("dialog .leaflet-map-pane").style.transform !==
          window.pickerTransform,
      );
      await picker()
        .getByRole("button", { name: "Use map centre", exact: true })
        .click();
      await picker()
        .getByLabel("Place name", { exact: true })
        .fill("Confirmed map point");
      await picker()
        .getByRole("button", { name: "Confirm location", exact: true })
        .click();
      uploadPolicy = "ok";
      await page
        .getByRole("button", { name: "Upload photo", exact: true })
        .click();
      await until(() => uploads.length === 3);
      assert.notEqual(uploads[2].key, uploads[1].key);
      assert.equal(uploads[2].location.place_name, "Confirmed map point");
      assert.equal(uploads[2].postal, "10001");
      assert.ok(
        uploads[2].location.longitude > place.longitude,
        "Keyboard panning must move the selected point",
      );

      await page.goto(`${origin}/me/uploads`);
      const edit = page.getByRole("button", { name: "Edit", exact: true });
      const save = () =>
        page
          .getByRole("form", { name: "Edit upload", exact: true })
          .getByRole("button", { name: "Save", exact: true })
          .click();
      await edit.click();
      await openPicker();
      assert.equal(
        await picker().getByLabel("Place name", { exact: true }).inputValue(),
        "Old place",
      );
      await picker()
        .getByLabel("Place name", { exact: true })
        .fill("Corrected place");
      await picker()
        .getByRole("button", { name: "Confirm location", exact: true })
        .click();
      assert.equal(edits.length, 0);
      await save();
      await until(() => edits.length === 1);
      assert.deepEqual(
        Object.keys(edits[0]),
        ["location"],
        "A pin correction must preserve legacy attribution",
      );
      assert.equal(edits[0].location.point.place_name, "Corrected place");
      await page
        .getByRole("form", { name: "Edit upload", exact: true })
        .waitFor({ state: "detached" });
      await edit.click();
      await page
        .getByRole("button", { name: "Remove location", exact: true })
        .click();
      await page.getByRole("button", { name: "Cancel", exact: true }).click();
      assert.equal(
        edits.length,
        1,
        "Cancelling must leave the saved pin unchanged",
      );
      await edit.click();
      await page
        .getByRole("button", { name: "Remove location", exact: true })
        .click();
      await save();
      await until(() => edits.length === 2);
      assert.deepEqual(edits[1], { location: { action: "clear" } });
      await page.getByRole("button", { name: "Timeline", exact: true }).click();
      await page
        .getByText("Old place: 40.70000, -74.00000 (accuracy unknown)", {
          exact: false,
        })
        .waitFor();
      assert.equal(
        await page.getByText('"latitude":', { exact: false }).count(),
        0,
        "History should explain the location instead of showing JSON",
      );
      assert.deepEqual(errors, []);
      console.log(
        `Contributor picker search, map keyboard, retry, and owner correction passed at ${width}px`,
      );
      await context.close();
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
