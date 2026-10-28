const assert = require("node:assert/strict");
const { chromium } = require("playwright");
const origin = process.env.TTL_PREVIEW_URL || "http://localhost:5173";
const city = (name) => ({
  id: name,
  name,
  country_code: "GB",
  center_lat: 51.5,
  center_lng: -0.12,
  default_zoom: 11,
  is_active: true,
});

(async () => {
  const browser = await chromium.launch({
    headless: true,
    channel: "chromium",
  });
  try {
    for (const width of [320, 390, 1440]) {
      const context = await browser.newContext({
        viewport: { width, height: 900 },
        serviceWorkers: "block",
      });
      await context.routeWebSocket("**/ws/feed", () => {});
      await context.route("https://fonts.googleapis.com/**", (route) =>
        route.abort(),
      );
      let initialFail = true,
        worldwideFail = false;
      const requests = [];
      await context.route("**/api/**", async (route) => {
        const url = new URL(route.request().url());
        let data = [],
          status = 200;
        if (url.pathname === "/api/v1/cities") {
          const query = url.searchParams.get("q");
          const discover = url.searchParams.get("discover") === "true";
          requests.push({ query, discover });
          if (!query) {
            status = initialFail ? 503 : 200;
            data = [city("Bengaluru")];
          } else if (discover) {
            status = worldwideFail ? 503 : 200;
            data = query === "Nowhere" ? [] : [city(query)];
            if (query === "Slow")
              await new Promise((resolve) => setTimeout(resolve, 1500));
          }
          if (status === 503)
            data = { error: "Service temporarily unavailable" };
        } else if (url.pathname === "/api/v1/letterings")
          data = { letterings: [], total: 0 };
        else if (url.pathname.endsWith("/summary"))
          data = { mapped: 0, unmapped: 0 };
        else if (url.pathname.endsWith("/notifications"))
          data = { items: [], total: 0, unread_count: 0 };
        await route.fulfill({ status, json: data }).catch(() => {});
      });
      const page = await context.newPage(),
        errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      page.setDefaultTimeout(10000);
      await page.goto(origin);
      await page
        .getByRole("button", { name: "All Cities", exact: true })
        .click();
      await page
        .getByText("Could not load saved cities.", { exact: false })
        .waitFor();
      initialFail = false;
      await page.getByRole("button", { name: "Retry cities" }).click();
      await page.getByRole("button", { name: /Bengaluru.*GB/ }).waitFor();
      const input = page.getByRole("textbox", {
        name: "Search city worldwide",
      });
      const local = page.waitForResponse((res) => {
        const url = new URL(res.url());
        return (
          url.pathname === "/api/v1/cities" &&
          url.searchParams.get("q") === "London"
        );
      });
      await input.fill("London");
      await local;
      await page.getByText("No matching cities", { exact: true }).waitFor();
      assert.equal(requests.filter((r) => r.discover).length, 0);
      const worldwide = page.getByRole("button", {
        name: "Search worldwide",
        exact: true,
      });
      await worldwide.focus();
      await page.keyboard.press("Enter");
      await page.getByRole("button", { name: /London.*GB/ }).click();
      assert.equal(requests.filter((r) => r.discover).length, 1);
      await page.getByRole("button", { name: "London", exact: true }).click();
      worldwideFail = true;
      await input.fill("Tokyo");
      await worldwide.click();
      await page
        .getByText("Could not search cities. Try again.", { exact: true })
        .waitFor();
      worldwideFail = false;
      await worldwide.click();
      await page.getByRole("button", { name: /Tokyo.*GB/ }).waitFor();
      await input.fill("Nowhere");
      await worldwide.click();
      await page
        .getByText("No cities found. Try another name.", { exact: true })
        .waitFor();

      const aborted = [];
      page.on("requestfailed", (request) => {
        if (request.url().includes("/cities")) aborted.push(request.url());
      });
      await input.fill("Slow");
      const pending = page.waitForRequest((request) => {
        const url = new URL(request.url());
        return (
          url.pathname === "/api/v1/cities" &&
          url.searchParams.get("q") === "Slow" &&
          url.searchParams.get("discover") === "true"
        );
      });
      await worldwide.click();
      await pending;
      await input.fill("Paris");
      await worldwide.click();
      await page.getByRole("button", { name: /Paris.*GB/ }).waitFor();
      await page.waitForTimeout(1600);
      assert.equal(
        await page.getByRole("button", { name: /Slow.*GB/ }).count(),
        0,
      );
      assert.ok(
        aborted.some((url) => url.includes("q=Slow")),
        "superseded lookup must abort",
      );

      await input.fill("Slow");
      const closing = page.waitForRequest(
        (request) =>
          request.url().includes("q=Slow") &&
          request.url().includes("discover=true"),
      );
      await worldwide.click();
      await closing;
      await page.keyboard.press("Escape");
      assert.equal(
        await page.getByRole("dialog", { name: "Choose a city" }).count(),
        0,
      );
      await page.getByRole("button", { name: "London", exact: true }).click();
      await input.fill("Bengaluru");
      await page.getByRole("button", { name: /Bengaluru.*GB/ }).waitFor();
      const bounds = await page
        .getByRole("dialog", { name: "Choose a city" })
        .boundingBox();
      assert.ok(
        bounds && bounds.x >= 0 && bounds.x + bounds.width <= width,
        "city picker must fit the viewport",
      );
      assert.deepEqual(errors, []);
      console.log(
        `PASS ${width}px: saved search, explicit discovery, keyboard, retry, empty results, cancellation and errors`,
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
