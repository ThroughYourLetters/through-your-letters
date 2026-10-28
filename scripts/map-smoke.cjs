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
  { id: "a", lat: 12.9716, lng: 77.5946, thumbnail: "" },
  { id: "b", lat: 12.971599999999999, lng: 77.5946, thumbnail: "" },
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
        empty = false,
        slow = false,
        release;
      const cityRequests = [];
      await context.route("**/api/v1/**", async (route) => {
        const url = new URL(route.request().url()),
          city = url.searchParams.get("city_id");
        let data = [];
        if (url.pathname.endsWith("/cities")) data = cities;
        if (url.pathname.endsWith("/geo/markers")) {
          cityRequests.push(city);
          if (slow && city === "city-a")
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
              : points;
        }
        if (url.pathname.endsWith("/geo/coverage"))
          data = city === "city-b" ? [] : coverage;
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
      const shared = page.getByTitle("2 uploads at this location", {
        exact: true,
      });
      await shared.waitFor();
      assert.equal(await page.locator(".leaflet-marker-icon").count(), 2);
      assert.equal(
        await page
          .getByTitle("1 upload at this location", { exact: true })
          .getAttribute("src")
          .then((src) => new URL(src, origin).origin),
        origin,
      );
      await shared.focus();
      await page.keyboard.press("Enter");
      assert.equal(
        await page
          .getByRole("link", { name: "View upload 1 of 2", exact: true })
          .getAttribute("href"),
        "/lettering/a",
      );
      assert.equal(
        await page
          .getByRole("link", { name: "View upload 2 of 2", exact: true })
          .getAttribute("href"),
        "/lettering/b",
      );
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
          .getByRole("button", { name: /^(All Cities|Bengaluru|London)$/ })
          .click();
        await page
          .getByRole("button", { name: new RegExp(`${name}.*(IN|GB)`) })
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
      await page
        .getByTitle("1 upload at this location", { exact: true })
        .waitFor();
      assert.equal(await page.locator(".leaflet-marker-icon").count(), 1);
      await onScreen();
      slow = true;
      await select("Bengaluru");
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
          .some((url) => new URL(url).searchParams.get("city_id") === "city-a"),
        "Superseded map request was not cancelled",
      );
      await page
        .getByTitle("1 upload at this location", { exact: true })
        .waitFor();
      await page.waitForTimeout(200);
      assert.equal(
        await page
          .getByTitle("2 uploads at this location", { exact: true })
          .count(),
        0,
      );
      empty = true;
      await select("Bengaluru");
      await page
        .getByText("No mapped uploads found.", { exact: true })
        .waitFor();
      assert.equal(await page.locator(".leaflet-marker-icon").count(), 0);
      failMarkers = true;
      await select("London");
      await page
        .getByRole("button", { name: "Retry map", exact: true })
        .waitFor();
      failMarkers = false;
      empty = false;
      await page
        .getByRole("button", { name: "Retry map", exact: true })
        .click();
      await page
        .getByTitle("1 upload at this location", { exact: true })
        .waitFor();
      await page
        .getByRole("button", { name: "Retry map", exact: true })
        .waitFor({ state: "hidden" });
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
        `PASS ${width}px: overlapping links, coverage groups, viewport, city races, empty results and retry`,
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
