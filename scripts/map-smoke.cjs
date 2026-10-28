const assert = require("node:assert/strict");
const fs = require("node:fs/promises");
const { chromium } = require("playwright");
const origin = process.env.TTL_PREVIEW_URL || "http://localhost:5173";
const config = require("../apps/api/config/runtime.json");
const tile =
  '<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256"><rect width="256" height="256" fill="#e8e3d8"/><path d="M0 50L256 130M80 0L150 256M0 200L256 180" stroke="white" stroke-width="12"/></svg>';
const image =
  "data:image/svg+xml," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128"><rect width="128" height="128" fill="#cc543a"/><text x="64" y="78" text-anchor="middle" fill="white" font-family="serif" font-size="40">TYPE</text></svg>',
  );
const cell = (id, lat, lng, count) => ({
  id,
  lat,
  lng,
  count,
  thumbnail: image,
  bounds: {
    west: lng - 0.0001,
    east: lng + 0.0001,
    south: lat - 0.0001,
    north: lat + 0.0001,
  },
});
const cells = [
  cell("dense", 12.9716, 77.5946, 40),
  cell("small", 12.975, 77.602, 3),
  cell("single", 12.962, 77.59, 1),
];
async function checkWorldCopies(browser) {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    serviceWorkers: "block",
    reducedMotion: "reduce",
  });
  try {
    await context.routeWebSocket("**/ws/feed", () => {});
    await context.route("https://fonts.googleapis.com/**", (route) =>
      route.abort(),
    );
    await context.route("https://*.tile.openstreetmap.org/**", (route) =>
      route.fulfill({ contentType: "image/svg+xml", body: tile }),
    );
    await context.route("**/api/v1/**", (route) => {
      const path = new URL(route.request().url()).pathname;
      let data = {};
      if (path.endsWith("/cities")) data = [];
      if (path.endsWith("/geo/density"))
        data = { cells: [cell("world", 20, 0, 40)], truncated: false };
      if (path.endsWith("/geo/summary")) data = { unmapped_count: 0 };
      if (path.endsWith("/geo/photos"))
        data = {
          items: [
            {
              id: "wrapped-photo",
              thumbnail: image,
              pin_code: "test",
              lat: 20,
              lng: 0,
            },
          ],
          next_cursor: null,
        };
      return route.fulfill({ json: data });
    });
    const page = await context.newPage();
    page.setDefaultTimeout(12000);
    await page.goto(`${origin}/map`);
    await page.getByRole("button", { name: "Zoom out", exact: true }).click();
    await Promise.all([
      page.waitForResponse((response) => {
        const url = new URL(response.url());
        return (
          url.pathname.endsWith("/geo/density") &&
          url.searchParams.get("zoom") === "0"
        );
      }),
      page.getByRole("button", { name: "Zoom out", exact: true }).click(),
    ]);
    const map = page.locator(".leaflet-container");
    const box = await map.boundingBox();
    const x = box.width / 2 + 256;
    await map.click({ position: { x, y: box.height / 2 } });
    await page
      .getByRole("region", { name: "Hotspot uploads" })
      .getByRole("link")
      .waitFor();
    const popup = await page.locator(".leaflet-popup").boundingBox();
    assert.ok(
      Math.abs(popup.x + popup.width / 2 - box.x - x) < 20,
      "Hotspot popup must open on the tapped world copy",
    );
    console.log(
      "PASS repeated worlds: visible heat spots open photos at the tapped location",
    );
  } finally {
    await context.close();
  }
}
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
        hasTouch: width < 500,
      });
      await context.routeWebSocket("**/ws/feed", () => {});
      await context.route("https://fonts.googleapis.com/**", (route) =>
        route.abort(),
      );
      await context.route("https://*.tile.openstreetmap.org/**", (route) =>
        route.fulfill({ contentType: "image/svg+xml", body: tile }),
      );
      let failDensity = false,
        badDensity = false,
        empty = false,
        failPhotos = true,
        failSummary = false,
        slow = false,
        release;
      const requests = [],
        aborted = [];
      await context.route("**/api/v1/**", async (route) => {
        const req = route.request(),
          url = new URL(req.url()),
          path = url.pathname;
        requests.push(url);
        let data = {};
        if (path.endsWith("/cities"))
          data = [
            {
              id: "city-a",
              name: "Bengaluru",
              country_code: "IN",
              center_lat: 12.9716,
              center_lng: 77.5946,
              default_zoom: 13,
              is_active: true,
            },
            {
              id: "city-b",
              name: "Tokyo",
              country_code: "JP",
              center_lat: 35.68,
              center_lng: 139.7,
              default_zoom: 12,
              is_active: true,
            },
          ];
        if (path.endsWith("/geo/density")) {
          if (slow && url.searchParams.get("city_id") === "city-a")
            await new Promise((resolve) => {
              release = resolve;
            });
          if (failDensity)
            return route.fulfill({
              status: 503,
              json: { error: "Unavailable" },
            });
          data = badDensity
            ? { cells: [{ ...cells[0], lat: 999 }], truncated: false }
            : {
                cells:
                  empty || url.searchParams.get("city_id") === "city-b"
                    ? []
                    : cells,
                truncated: false,
              };
        }
        if (path.endsWith("/geo/summary")) {
          if (failSummary)
            return route.fulfill({
              status: 503,
              json: { error: "Unavailable" },
            });
          data = { unmapped_count: 7 };
        }
        if (path.endsWith("/geo/photos")) {
          if (failPhotos)
            return route.fulfill({
              status: 503,
              json: { error: "Unavailable" },
            });
          const after = url.searchParams.get("after");
          data = {
            items: Array.from({ length: after ? 10 : 30 }, (_, index) => ({
              id: `photo-${index + (after ? 30 : 0)}`,
              pin_code: "560025",
              thumbnail: image,
              lat: 12.9716,
              lng: 77.5946,
            })),
            next_cursor: after ? null : "after-30",
          };
        }
        await route.fulfill({ status: 200, json: data });
      });
      const page = await context.newPage(),
        errors = [];
      page.setDefaultTimeout(12000);
      page.on("pageerror", (error) => {
        console.error(error.stack);
        errors.push(error.message);
      });
      page.on("requestfailed", (req) => {
        if (req.url().includes("/geo/density")) aborted.push(req.url());
      });
      const chooseCity = async (name) => {
        await page
          .getByRole("button", {
            name: name === "Bengaluru" ? "All Cities" : "Bengaluru",
            exact: true,
          })
          .click();
        await page
          .getByRole("button", {
            name: new RegExp(`${name}.*${name === "Bengaluru" ? "IN" : "JP"}`),
          })
          .click();
      };
      await page.goto(`${origin}/map`);
      await chooseCity("Bengaluru");
      await page.getByText("Browse visible areas", { exact: true }).click();
      await page
        .getByRole("button", { name: "Area 1: 40 uploads", exact: true })
        .waitFor();
      await page
        .getByText(
          "7 approved uploads are not shown because their locations need review.",
          { exact: true },
        )
        .waitFor();
      const canvas = page.locator(".leaflet-overlay-pane canvas");
      await canvas.waitFor();
      await page.waitForFunction(() => {
        const canvas = document.querySelector(".leaflet-overlay-pane canvas");
        if (!canvas) return false;
        return canvas
          .getContext("2d")
          .getImageData(0, 0, canvas.width, canvas.height)
          .data.some((v, i) => i % 4 === 3 && v > 0);
      });
      const colors = await canvas.evaluate((canvas) => {
        const bytes = canvas
          .getContext("2d")
          .getImageData(0, 0, canvas.width, canvas.height).data;
        let blue = 0,
          red = 0,
          green = 0;
        for (let i = 0; i < bytes.length; i += 4) {
          if (bytes[i + 3] < 20) continue;
          if (bytes[i + 2] > 160 && bytes[i] < 120) blue++;
          if (bytes[i] > 180 && bytes[i + 1] < 120 && bytes[i + 2] < 120) red++;
          if (bytes[i + 1] > 140 && bytes[i] < 180 && bytes[i + 2] < 180)
            green++;
        }
        return { blue, red, green };
      });
      assert.ok(
        colors.blue > 20 && colors.green > 20 && colors.red > 20,
        JSON.stringify(colors),
      );
      await fs.mkdir("/tmp/ttl-heatmap-artifacts", { recursive: true });
      await page.getByText("Browse visible areas", { exact: true }).click();
      await page.evaluate(() =>
        window.scrollTo({ top: 0, behavior: "instant" }),
      );
      await page.screenshot({
        path: `/tmp/ttl-heatmap-artifacts/heatmap-${width}.png`,
      });
      if (width < 768) {
        const near = await page
          .getByRole("button", { name: "Near me", exact: true })
          .boundingBox();
        const nav = await page.locator(".mobile-bottom-nav").boundingBox();
        assert.ok(
          near.y + near.height <= nav.y,
          "Map location control overlaps bottom navigation",
        );
      }
      await page.getByText("Browse visible areas", { exact: true }).click();
      assert.equal(await page.getByTitle("Show coverage").count(), 0);
      assert.equal(
        requests.some(
          (url) =>
            url.pathname.endsWith("/geo/coverage") ||
            url.pathname.endsWith("/geo/markers"),
        ),
        false,
      );
      const mapBox = await page.locator(".leaflet-container").boundingBox();
      await page
        .locator(".leaflet-container")
        .click({ position: { x: mapBox.width / 2, y: mapBox.height / 2 } });
      await page
        .getByRole("button", { name: "Retry photos", exact: true })
        .waitFor();
      await page.getByRole("button", { name: "Close popup" }).click();
      await page
        .getByRole("button", { name: "Area 1: 40 uploads", exact: true })
        .focus();
      await page.keyboard.press("Enter");
      await page
        .getByRole("button", { name: "Retry photos", exact: true })
        .waitFor();
      failPhotos = false;
      await page
        .getByRole("button", { name: "Retry photos", exact: true })
        .click();
      const hotspot = page.getByRole("region", { name: "Hotspot uploads" });
      await hotspot.getByRole("link").first().waitFor();
      assert.equal(await hotspot.getByRole("link").count(), 30);
      await hotspot.getByRole("button", { name: "Next photos" }).click();
      await hotspot.locator('a[href="/lettering/photo-30"]').waitFor();
      assert.equal(
        await hotspot.getByRole("link").first().getAttribute("href"),
        "/lettering/photo-30",
      );
      assert.equal(await hotspot.getByRole("link").count(), 10);
      assert.ok(
        await hotspot.getByRole("button", { name: "Next photos" }).isDisabled(),
      );
      await hotspot.getByRole("button", { name: "Previous photos" }).click();
      await hotspot.getByRole("link").first().waitFor();
      await page.getByRole("button", { name: "Close popup" }).click();
      await page.getByRole("button", { name: "Zoom in" }).click();
      await page.getByRole("button", { name: "Zoom in" }).click();
      await page
        .getByTitle("40 uploads in this area", { exact: true })
        .waitFor();
      await page.getByTitle("40 uploads in this area", { exact: true }).focus();
      await page.keyboard.press("Enter");
      await hotspot.waitFor();
      await page.getByRole("button", { name: "Close popup" }).click();
      await Promise.all([
        page.waitForResponse(
          (response) =>
            response.url().includes("/geo/density") &&
            new URL(response.url()).searchParams.get("script") === "Latin",
        ),
        page
          .getByRole("button", { name: "English / Latin", exact: true })
          .click(),
      ]);
      await Promise.all([
        page.waitForResponse(
          (response) =>
            response.url().includes("/geo/density") &&
            new URL(response.url()).searchParams.get("style") ===
              "Hand-painted",
        ),
        page.getByLabel("Style", { exact: true }).selectOption("Hand-painted"),
      ]);
      const map = page.locator(".leaflet-container");
      const box = await map.boundingBox();
      const before = requests.filter((url) =>
        url.pathname.endsWith("/geo/density"),
      ).length;
      await map.focus();
      await page.keyboard.press("ArrowRight");
      await page.keyboard.press("ArrowRight");
      await page.waitForTimeout(config.map.move_debounce_ms + 400);
      const after = requests.filter((url) =>
        url.pathname.endsWith("/geo/density"),
      ).length;
      assert.ok(after - before <= 2);
      await context.grantPermissions(["geolocation"]);
      await context.setGeolocation({ latitude: 12.975, longitude: 77.602 });
      await page.getByRole("button", { name: "Near me", exact: true }).click();
      await page.waitForTimeout(config.map.move_debounce_ms + 400);
      await map.focus();
      await page.keyboard.press("ArrowRight");
      await page.getByRole("button", { name: "Near me", exact: true }).click();
      await page.waitForTimeout(config.map.move_debounce_ms + 400);
      await fs.mkdir("/tmp/ttl-heatmap-artifacts", { recursive: true });
      await page.screenshot({
        path: `/tmp/ttl-heatmap-artifacts/map-${width}.png`,
        fullPage: true,
      });
      for (const [code, message] of [
        [1, "Location permission was denied. Check your browser settings."],
        [2, "Your location is unavailable. Try again."],
        [3, "Location request timed out. Try again."],
      ]) {
        await page.evaluate(
          (code) =>
            Object.defineProperty(navigator, "geolocation", {
              configurable: true,
              value: {
                getCurrentPosition(_success, error, options) {
                  window.requestedLocationTimeout = options.timeout;
                  error({ code, PERMISSION_DENIED: 1, TIMEOUT: 3 });
                },
              },
            }),
          code,
        );
        await page
          .getByRole("button", { name: "Near me", exact: true })
          .click();
        await page.getByText(message, { exact: true }).waitFor();
        assert.equal(
          await page.evaluate(() => window.requestedLocationTimeout),
          config.web.geolocation_timeout_seconds * 1000,
        );
      }
      await page.evaluate(() =>
        Object.defineProperty(navigator, "geolocation", {
          configurable: true,
          value: undefined,
        }),
      );
      await page.getByRole("button", { name: "Near me", exact: true }).click();
      await page
        .getByText("Location is unavailable in this browser.", { exact: true })
        .waitFor();
      // Exercise fresh loads outside query caches.
      failDensity = true;
      await page.reload();
      await page.getByRole("button", { name: "Retry map" }).waitFor();
      failDensity = false;
      await page.getByRole("button", { name: "Retry map" }).click();
      await page.getByText("Browse visible areas", { exact: true }).click();
      await page
        .getByRole("button", { name: "Area 1: 40 uploads", exact: true })
        .waitFor();
      badDensity = true;
      await page.reload();
      await page.getByRole("button", { name: "Retry map" }).waitFor();
      badDensity = false;
      empty = true;
      await page.reload();
      await page
        .getByText("No mapped uploads found in this view.", { exact: true })
        .waitFor();
      empty = false;
      failSummary = true;
      await page.reload();
      await page
        .getByRole("button", { name: "Retry location status" })
        .waitFor();
      failSummary = false;
      await page.getByRole("button", { name: "Retry location status" }).click();
      await page
        .getByText(
          "7 approved uploads are not shown because their locations need review.",
          { exact: true },
        )
        .waitFor();
      slow = true;
      await map.focus();
      await page.keyboard.press("ArrowRight");
      await page.waitForTimeout(config.map.move_debounce_ms + 100);
      const heldRequest = page.waitForRequest(
        (request) =>
          request.url().includes("/geo/density") &&
          new URL(request.url()).searchParams.get("city_id") === "city-a",
      );
      await chooseCity("Bengaluru");
      await heldRequest;
      await page
        .getByRole("button", { name: "Bengaluru", exact: true })
        .click();
      await page.getByRole("button", { name: /Tokyo.*JP/ }).click();
      slow = false;
      if (release) release();
      await page
        .getByText("No mapped uploads found in this view.", { exact: true })
        .waitFor();
      assert.equal(
        await page
          .getByRole("button", { name: "Area 1: 40 uploads", exact: true })
          .count(),
        0,
      );
      assert.equal(
        await page.evaluate(
          () => document.documentElement.scrollWidth > innerWidth,
        ),
        false,
      );
      assert.deepEqual(errors, []);
      console.log(
        `PASS ${width}px: canvas gradient, hotspot photos, keyboard, zoom, filters, pan, geolocation, retries, malformed data, empty results and city races`,
      );
      await context.close();
    }
    await checkWorldCopies(browser);
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
