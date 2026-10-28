const assert = require("node:assert/strict");
const fs = require("node:fs");
const ts = require("typescript");
const { chromium } = require("playwright");
const origin = process.env.TTL_PREVIEW_URL || "http://localhost:5173";
const source = ts.transpileModule(
  fs.readFileSync("apps/web/src/lib/mapMarkers.ts", "utf8"),
  {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
    },
  },
).outputText;
const exported = {};
new Function("exports", source)(exported);
const points = [
  {
    id: "a",
    lat: 12.9716,
    lng: 77.5946,
    thumbnail: "",
    location_source: "postal_code",
  },
  {
    id: "b",
    lat: 12.971599999999999,
    lng: 77.5946,
    thumbnail: "",
    location_source: "postal_code",
  },
  { id: "c", lat: 12.98, lng: 77.62, thumbnail: "" },
];
assert.deepEqual(
  exported.groupMarkers(points).map((group) => group.map((p) => p.id)),
  [["a", "b"], ["c"]],
);
assert.equal(
  exported.groupMarkers([
    { lat: NaN, lng: 0 },
    { lat: 91, lng: 0 },
    { lat: 0, lng: Infinity },
    { lat: 0, lng: -181 },
  ]).length,
  0,
);
assert.equal(
  exported.groupMarkers([
    { lat: -0.00000001, lng: 0 },
    { lat: 0, lng: -0 },
  ]).length,
  1,
);
const cities = [
  {
    id: "city-a",
    name: "Bengaluru",
    country_code: "IN",
    center_lat: 12.97,
    center_lng: 77.59,
    default_zoom: 11,
    is_active: true,
  },
  {
    id: "city-b",
    name: "London",
    country_code: "GB",
    center_lat: 51.5,
    center_lng: -0.1,
    default_zoom: 11,
    is_active: true,
  },
];
cities.push({
  id: "city-c",
  name: "Tokyo",
  country_code: "JP",
  center_lat: 35.68,
  center_lng: 139.7,
  default_zoom: 11,
  is_active: true,
});
const coverage = [
  {
    city_id: "city-a",
    city_name: "Bengaluru",
    pin_code: "560025",
    lat: 12.9716,
    lng: 77.5946,
    count: 10,
  },
  {
    city_id: "city-a",
    city_name: "Bengaluru",
    pin_code: "560073",
    lat: 12.971599999999999,
    lng: 77.5946,
    count: 3,
  },
];
const tile =
  '<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256"><path fill="#eee" d="M0 0h256v256H0z"/><path stroke="#ddd" d="M0 128h256M128 0v256"/></svg>';
(async () => {
  const browser = await chromium.launch({
    headless: true,
    channel: "chromium",
  });
  try {
    for (const width of [390, 1440]) {
      const context = await browser.newContext({
        viewport: { width, height: 1000 },
        serviceWorkers: "block",
        reducedMotion: "reduce",
      });
      await context.route("https://fonts.googleapis.com/**", (r) => r.abort());
      await context.route("https://*.tile.openstreetmap.org/**", (r) =>
        r.fulfill({ contentType: "image/svg+xml", body: tile }),
      );
      await context.routeWebSocket("**/ws/feed", () => {});
      let failMarkers = false,
        malformedMarkers = false,
        failCoverage = false,
        empty = false,
        slow = false,
        release;
      const manyPoints = [
        ...points.slice(0, 2),
        ...Array.from({ length: 10 }, (_, index) => ({
          ...points[0],
          id: `extra-${index}`,
        })),
        points[2],
      ];
      manyPoints[0] = {
        ...manyPoints[0],
        thumbnail: `data:image/svg+xml,${encodeURIComponent(tile)}`,
      };
      const cityRequests = [];
      await context.route("**/api/v1/**", async (route) => {
        const url = new URL(route.request().url()),
          city = url.searchParams.get("city_id");
        let data = [];
        if (url.pathname.endsWith("/cities")) data = cities;
        if (url.pathname.endsWith("/geo/markers")) {
          cityRequests.push(city);
          if (slow && city === "city-c")
            await new Promise((resolve) => {
              release = resolve;
            });
          if (failMarkers)
            return route
              .fulfill({ status: 503, json: { error: "Unavailable" } })
              .catch(() => {});
          data = empty
            ? []
            : city === "city-b"
              ? [{ id: "london", lat: 51.5, lng: -0.1, thumbnail: "" }]
              : manyPoints;
        }
        if (malformedMarkers && url.pathname.endsWith("/geo/markers"))
          data = { unexpected: "response" };
        if (url.pathname.endsWith("/geo/coverage")) {
          if (failCoverage)
            return route
              .fulfill({ status: 503, json: { error: "Unavailable" } })
              .catch(() => {});
          data = city === "city-b" ? [] : coverage;
        }
        if (url.pathname === "/api/v1/letterings")
          data = { letterings: [], total: 0 };
        await route.fulfill({ json: data }).catch(() => {});
      });
      const page = await context.newPage();
      const errors = [];
      const cancelled = [];
      page.on("requestfailed", (request) => {
        if (
          request.url().includes("/geo/markers") &&
          request.failure()?.errorText.includes("ABORTED")
        )
          cancelled.push(request.url());
      });
      page.on("pageerror", (e) => errors.push(e.message));
      page.setDefaultTimeout(15000);
      await page.goto(`${origin}/map`);
      const shared = page.getByTitle("12 uploads at this pin", {
        exact: true,
      });
      await shared.waitFor();
      assert.equal(await page.locator(".leaflet-marker-icon").count(), 2);
      const iconSource = await page
        .getByTitle("1 upload at this pin", { exact: true })
        .getAttribute("src");
      assert.ok(
        iconSource.startsWith("data:image/") ||
          new URL(iconSource, origin).origin === origin,
        "Marker icon requires an external host",
      );
      await shared.focus();
      await page.keyboard.press("Enter");
      assert.ok(
        (await page
          .getByText("Approximate postal area", { exact: true })
          .count()) > 0,
      );
      assert.equal(
        await page
          .getByRole("link", { name: "View upload 1 of 12", exact: true })
          .getAttribute("href"),
        "/lettering/a",
      );
      assert.equal(
        await page
          .getByRole("link", { name: "View upload 2 of 12", exact: true })
          .getAttribute("href"),
        "/lettering/b",
      );
      await page.waitForFunction(() => {
        const title = document
          .querySelector(".leaflet-popup-content p")
          ?.getBoundingClientRect();
        const control = document
          .querySelector(".leaflet-control-zoom")
          ?.getBoundingClientRect();
        return (
          title &&
          control &&
          (title.x >= control.right ||
            title.y >= control.bottom ||
            title.right <= control.x ||
            title.bottom <= control.y)
        );
      });
      const popupHeading = await page
        .getByText("12 uploads at this pin", { exact: true })
        .boundingBox();
      const zoomControl = await page
        .locator(".leaflet-control-zoom")
        .boundingBox();
      assert.ok(
        popupHeading.x >= zoomControl.x + zoomControl.width ||
          popupHeading.y >= zoomControl.y + zoomControl.height ||
          popupHeading.x + popupHeading.width <= zoomControl.x ||
          popupHeading.y + popupHeading.height <= zoomControl.y,
        "Map controls obscure popup heading",
      );
      const uploadLinks = page
        .locator(".leaflet-popup-content")
        .getByRole("link");
      assert.equal(await uploadLinks.count(), 12);
      const finalUpload = page.getByRole("link", {
        name: "View upload 12 of 12",
        exact: true,
      });
      await finalUpload.scrollIntoViewIfNeeded();
      assert.equal(
        await finalUpload.getAttribute("href"),
        "/lettering/extra-9",
      );
      assert.ok(await finalUpload.isVisible());
      assert.equal(
        await page
          .locator(".leaflet-popup-content img")
          .first()
          .getAttribute("loading"),
        "lazy",
      );
      await page.screenshot({
        path: `/tmp/ttl-map-shared-${width}.png`,
        fullPage: true,
      });
      await page.keyboard.press("Escape");
      await page.getByTitle("Show coverage", { exact: true }).click();
      const circle = page.locator(
        ".leaflet-overlay-pane path.leaflet-interactive",
      );
      const circleBox = await circle.boundingBox();
      await circle.click({
        position: { x: circleBox.width * 0.85, y: circleBox.height / 2 },
      });
      const popup = page.locator(".leaflet-popup-content");
      await popup
        .getByText("Bengaluru 560025: 10 uploads", { exact: true })
        .waitFor();
      await popup
        .getByText("Bengaluru 560073: 3 uploads", { exact: true })
        .waitFor();
      await page.keyboard.press("Escape");
      await page.getByTitle("Show coverage", { exact: true }).click();
      const select = async (name) => {
        await page
          .getByRole("button", {
            name: /^(All Cities|Bengaluru|London|Tokyo)$/,
          })
          .click();
        await page
          .getByRole("button", { name: new RegExp(`${name}.*(IN|GB|JP)`) })
          .click();
      };
      await select("Bengaluru");
      await shared.waitFor();
      const onScreen = async () => {
        const map = await page.locator(".leaflet-container").boundingBox();
        for (const pin of await page.locator(".leaflet-marker-icon").all()) {
          const box = await pin.boundingBox();
          assert.ok(
            box.x >= map.x &&
              box.x + box.width <= map.x + map.width &&
              box.y >= map.y &&
              box.y + box.height <= map.y + map.height,
            "Marker is outside viewport",
          );
        }
      };
      await onScreen();
      await select("London");
      await page.getByTitle("1 upload at this pin", { exact: true }).waitFor();
      assert.equal(await page.locator(".leaflet-marker-icon").count(), 1);
      await onScreen();
      const requestsBeforeRevisit = cityRequests.length;
      await select("Bengaluru");
      await shared.waitFor();
      await select("London");
      await page.getByTitle("1 upload at this pin", { exact: true }).waitFor();
      assert.equal(
        cityRequests.length,
        requestsBeforeRevisit,
        "Fresh city data was fetched again",
      );
      slow = true;
      await select("Tokyo");
      await page.waitForFunction(
        () =>
          document.querySelector('[role="status"]') !== null ||
          document.querySelector(".leaflet-marker-icon") !== null,
      );
      // Keep the previous city response pending.
      for (let n = 0; !release && n < 50; n++) await page.waitForTimeout(20);
      assert.ok(release, "Delayed request did not start");
      const cancelledBefore = cancelled.length;
      await select("London");
      release();
      slow = false;
      for (let n = 0; cancelled.length === cancelledBefore && n < 50; n++)
        await page.waitForTimeout(20);
      assert.ok(
        cancelled
          .slice(cancelledBefore)
          .some((url) => new URL(url).searchParams.get("city_id") === "city-c"),
        "Superseded map request was not cancelled",
      );
      await page.getByTitle("1 upload at this pin", { exact: true }).waitFor();
      await page.waitForTimeout(200);
      assert.equal(
        await page
          .getByTitle("12 uploads at this pin", { exact: true })
          .count(),
        0,
      );
      empty = true;
      await page.reload();
      await page
        .getByText("No mapped uploads found.", { exact: true })
        .waitFor();
      assert.equal(await page.locator(".leaflet-marker-icon").count(), 0);
      failMarkers = true;
      await page.reload();
      await page
        .getByRole("button", { name: "Retry map", exact: true })
        .waitFor();
      failMarkers = false;
      empty = false;
      await page
        .getByRole("button", { name: "Retry map", exact: true })
        .click();
      await page.getByTitle("1 upload at this pin", { exact: true }).waitFor();
      await page
        .getByRole("button", { name: "Retry map", exact: true })
        .waitFor({ state: "hidden" });
      malformedMarkers = true;
      await page.reload();
      await page
        .getByRole("button", { name: "Retry map", exact: true })
        .waitFor();
      malformedMarkers = false;
      await page
        .getByRole("button", { name: "Retry map", exact: true })
        .click();
      await page
        .getByRole("button", { name: "Retry map", exact: true })
        .waitFor({ state: "hidden" });
      failCoverage = true;
      await page.reload();
      await page.getByTitle("Show coverage", { exact: true }).click();
      const mapPanel = page.locator(".leaflet-container").locator("..");
      await mapPanel
        .getByRole("button", { name: "Retry coverage", exact: true })
        .waitFor();
      failCoverage = false;
      await mapPanel
        .getByRole("button", { name: "Retry coverage", exact: true })
        .click();
      await mapPanel
        .getByRole("button", { name: "Retry coverage", exact: true })
        .waitFor({ state: "hidden" });
      await page.getByTitle("Show coverage", { exact: true }).click();

      const setLocation = async (kind) =>
        page.evaluate((kind) => {
          Object.defineProperty(navigator, "geolocation", {
            configurable: true,
            value:
              kind === "unsupported"
                ? undefined
                : {
                    getCurrentPosition(success, failure, options) {
                      window.locationTimeout = options.timeout;
                      if (kind === "pending") window.finishLocation = success;
                      else if (kind === "success")
                        success({
                          coords: { latitude: 51.5, longitude: -0.1 },
                        });
                      else
                        failure({
                          code:
                            kind === "denied" ? 1 : kind === "timeout" ? 3 : 2,
                          PERMISSION_DENIED: 1,
                          TIMEOUT: 3,
                        });
                    },
                  },
          });
        }, kind);
      for (const [kind, message] of [
        ["unsupported", "Location is unavailable in this browser."],
        [
          "denied",
          "Location permission was denied. Check your browser settings.",
        ],
        ["unavailable", "Your location is unavailable. Try again."],
        ["timeout", "Location request timed out. Try again."],
      ]) {
        await setLocation(kind);
        await page.getByTitle("Near me", { exact: true }).click();
        await page.getByRole("alert").filter({ hasText: message }).waitFor();
      }
      const limits = JSON.parse(
        fs.readFileSync("apps/api/config/runtime.json", "utf8"),
      );
      assert.equal(
        await page.evaluate(() => window.locationTimeout),
        limits.web.geolocation_timeout_seconds * 1000,
      );
      await setLocation("pending");
      await page.getByTitle("Near me", { exact: true }).click();
      await select("London");
      await page.getByTitle("1 upload at this pin", { exact: true }).waitFor();
      await page.evaluate(() =>
        window.finishLocation({
          coords: { latitude: 12.97, longitude: 77.59 },
        }),
      );
      await page.waitForTimeout(100);
      assert.equal(
        await page
          .locator('.leaflet-overlay-pane path[stroke="#3b82f6"]')
          .count(),
        0,
      );
      await setLocation("success");
      await page.getByTitle("Near me", { exact: true }).click();
      await page
        .locator('.leaflet-overlay-pane path[stroke="#3b82f6"]')
        .waitFor();
      const locationAtCenter = (centered) => {
        const marker = document
          .querySelector('.leaflet-overlay-pane path[stroke="#3b82f6"]')
          ?.getBoundingClientRect();
        const map = document
          .querySelector(".leaflet-container")
          ?.getBoundingClientRect();
        if (!marker || !map) return false;
        const distance = Math.hypot(
          marker.x + marker.width / 2 - map.x - map.width / 2,
          marker.y + marker.height / 2 - map.y - map.height / 2,
        );
        return centered ? distance < 5 : distance > 30;
      };
      await page.waitForFunction(locationAtCenter, true);
      await page.locator(".leaflet-container").focus();
      await page.keyboard.press("ArrowRight");
      await page.keyboard.press("ArrowDown");
      await page.waitForFunction(locationAtCenter, false);
      const beforeFocus = cityRequests.length;
      await page.evaluate(() =>
        document.dispatchEvent(new Event("visibilitychange")),
      );
      await page.waitForTimeout(100);
      assert.equal(
        cityRequests.length,
        beforeFocus,
        "Fresh map data refetched on focus",
      );
      await page.getByTitle("Near me", { exact: true }).click();
      await page.waitForFunction(locationAtCenter, true);
      assert.equal(await page.getByRole("alert").count(), 0);
      assert.ok(
        cityRequests.includes("city-a") && cityRequests.includes("city-b"),
      );
      assert.deepEqual(errors, []);
      assert.ok(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth + 1,
        ),
      );
      await page.screenshot({
        path: `/tmp/ttl-map-${width}.png`,
        fullPage: true,
      });
      console.log(
        `PASS ${width}px: overlapping links, coverage groups, viewport, city races, empty results, retries and geolocation outcomes`,
      );
      await context.close();
    }
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
