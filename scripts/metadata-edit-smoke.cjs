const assert = require("node:assert/strict");
const { chromium } = require("playwright");
const origin = process.env.TTL_PREVIEW_URL || "http://localhost:5173";
const image =
  "data:image/svg+xml," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="#cc543a"/></svg>',
  );

(async () => {
  const browser = await chromium.launch({
    headless: true,
    channel: "chromium",
  });
  try {
    for (const width of [390, 1440]) {
      const context = await browser.newContext({
        viewport: { width, height: 900 },
        serviceWorkers: "block",
        reducedMotion: "reduce",
      });
      await context.addInitScript(() =>
        sessionStorage.setItem("ttl_user_token", "metadata-fixture"),
      );
      await context.routeWebSocket("**/ws/feed", () => {});
      await context.route("https://fonts.googleapis.com/**", (route) =>
        route.abort(),
      );
      await context.route("https://*.tile.openstreetmap.org/**", (route) =>
        route.fulfill({
          contentType: "image/svg+xml",
          body: '<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256"/>',
        }),
      );
      let current = {
        id: "metadata-photo",
        pin_code: "560025",
        description: "Original description",
        contributor_tag: "Owner",
        detected_text: "Editable sign",
        image_url: image,
        thumbnail_small: image,
        thumbnail_urls: { small: image },
        status: "APPROVED",
        location_source: "postal_code",
        location: { type: "Point", coordinates: [0, 20] },
        created_at: "2026-10-01T00:00:00Z",
        updated_at: "2026-10-01T00:00:00Z",
        likes_count: 0,
        comments_count: 0,
      };
      let failEdit = true,
        densityRequests = 0,
        galleryRequests = 0;
      const patches = [];
      await context.route("**/api/v1/**", async (route) => {
        const request = route.request(),
          path = new URL(request.url()).pathname;
        let data = [];
        if (path === "/api/v1/auth/me")
          data = {
            id: "owner",
            email: "owner@example.test",
            role: "USER",
            created_at: current.created_at,
          };
        if (path === "/api/v1/letterings") {
          galleryRequests++;
          data = { letterings: [current], total: 1 };
        }
        if (path === "/api/v1/me/letterings")
          data = { items: [current], total: 1, limit: 50, offset: 0 };
        if (
          path === "/api/v1/me/letterings/metadata-photo" &&
          request.method() === "PATCH"
        ) {
          const patch = request.postDataJSON();
          patches.push(patch);
          if (failEdit)
            return route.fulfill({
              status: 500,
              json: { error: "Could not save upload." },
            });
          const changed = patch.pin_code !== current.pin_code;
          current = {
            ...current,
            ...patch,
            ...(changed
              ? { location: null, location_source: "unresolved" }
              : {}),
          };
          data = current;
        }
        if (path === "/api/v1/geo/density") {
          densityRequests++;
          data = {
            cells: current.location
              ? [
                  {
                    id: "area",
                    count: 1,
                    lat: 20,
                    lng: 0,
                    thumbnail: image,
                    bounds: { west: -1, south: 19, east: 1, north: 21 },
                  },
                ]
              : [],
            truncated: false,
          };
        }
        if (path === "/api/v1/geo/summary")
          data = { unmapped_count: current.location ? 0 : 1 };
        await route.fulfill({ json: data });
      });
      const page = await context.newPage(),
        errors = [];
      page.setDefaultTimeout(10000);
      page.on("pageerror", (error) => errors.push(error.message));
      const navigate = async (name) =>
        page
          .getByRole("link", { name, exact: true })
          .filter({ visible: true })
          .click();
      const uploads = async () => {
        if (width < 1024)
          await page
            .getByRole("button", { name: "Open menu", exact: true })
            .click();
        await navigate("My Uploads");
        await page.getByRole("button", { name: "Edit", exact: true }).click();
      };
      await page.goto(origin);
      await page
        .getByRole("button", { name: "Open Editable sign", exact: true })
        .first()
        .waitFor();
      await navigate("Map");
      await page.getByText("Browse visible areas", { exact: true }).waitFor();
      assert.equal(densityRequests, 1);
      await uploads();
      await page
        .getByText(
          "Changing the postal code removes the map location until it is reviewed.",
          { exact: true },
        )
        .waitFor();
      const postal = page.getByRole("textbox", { name: "Postal or ZIP code" });
      await postal.fill("560001");
      await page.getByRole("button", { name: "Save", exact: true }).click();
      await page.getByText("Could not save upload.", { exact: true }).waitFor();
      assert.equal(await postal.inputValue(), "560001");
      assert.equal(current.pin_code, "560025");
      failEdit = false;
      await page.getByRole("button", { name: "Save", exact: true }).click();
      await page
        .getByText("Upload updated. Its location needs review.", {
          exact: true,
        })
        .waitFor();
      assert.equal(patches.length, 2);
      assert.equal(patches[1].pin_code, "560001");
      await navigate("Map");
      await page
        .getByText("No mapped uploads found in this view.", { exact: true })
        .waitFor();
      await page
        .getByText(
          "1 approved upload is not shown because its location needs review.",
          { exact: true },
        )
        .waitFor();
      assert.ok(
        densityRequests >= 2,
        "Postal edits must invalidate cached map locations",
      );
      await navigate("Explore");
      await page
        .locator("article")
        .getByText(/^560001\b/)
        .first()
        .waitFor();
      assert.ok(
        galleryRequests >= 2,
        "Postal edits must invalidate the cached gallery",
      );
      await uploads();
      await page.locator("textarea").fill("Updated description");
      await page.getByRole("button", { name: "Save", exact: true }).click();
      await page.getByText("Upload updated", { exact: true }).waitFor();
      assert.equal(current.description, "Updated description");
      assert.deepEqual(errors, []);
      console.log(
        `PASS ${width}px: edit errors preserve drafts; postal edits refresh map and gallery caches and explain location review`,
      );
      await context.close();
    }
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
