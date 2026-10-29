// Real application modules and UI, with every API/WebSocket request intercepted.
const assert = require("node:assert/strict");
const path = require("node:path");
const { chromium } = require("playwright");
const root = path.resolve(__dirname, "..");
const user = (id) => ({
  id,
  email: `${id}@example.com`,
  display_name: id.toUpperCase(),
  role: "USER",
  created_at: "2026-01-01",
});
const upload = (id) => ({
  id,
  detected_text: `${id.toUpperCase()} private upload`,
  image_url:
    "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==",
  thumbnail_small: "",
  pin_code: "560001",
  contributor_tag: id,
  status: "PENDING",
  likes_count: 0,
  comments_count: 0,
  report_count: 0,
  created_at: "2026-01-01",
  updated_at: "2026-01-01",
});
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
    const context = await browser.newContext({ serviceWorkers: "block" });
    await context.route("https://fonts.googleapis.com/**", (r) => r.abort());
    await context.routeWebSocket("**/ws/feed", () => {});
    const held = [];
    const likeCredentials = [];
    let releaseLogin;
    let loginStarted;
    const waitingLogin = new Promise((resolve) => {
      loginStarted = resolve;
    });
    await context.route("**/api/v1/**", async (route) => {
      const req = route.request();
      const url = new URL(req.url());
      const account = (req.headers().authorization || "Bearer a-token")
        .replace("Bearer ", "")
        .split("-")[0];
      if (url.pathname.endsWith("/like")) {
        likeCredentials.push(req.headers().authorization || null);
        return route.fulfill({ json: { liked: true, likes_count: 1 } });
      }
      if (url.pathname.endsWith("/auth/login")) {
        const id = req.postDataJSON().email.split("@")[0];
        if (id === "slow") {
          loginStarted();
          await new Promise((resolve) => {
            releaseLogin = resolve;
          });
        }
        return route.fulfill({
          json: { token: `${id}-token`, user: user(id) },
        });
      }
      if (url.searchParams.get("offset") === "999") {
        held.push(route);
        return;
      }
      if (url.pathname.includes("/admin/"))
        return route.fulfill({ status: 401, json: { error: "Admin expired" } });
      let data = [];
      if (url.pathname.endsWith("/auth/me")) data = user(account);
      if (url.pathname.endsWith("/me/letterings"))
        data = { items: [upload(account)], total: 1, limit: 50, offset: 0 };
      if (url.pathname === "/api/v1/letterings")
        data = { letterings: [], total: 0 };
      return route.fulfill({ json: data });
    });
    const page = await context.newPage();
    page.setDefaultTimeout(20000);
    await page.goto(origin);
    await page.evaluate(async () => {
      const { useAuthStore } = await import("/src/store/useAuthStore.ts");
      await useAuthStore.getState().login("a@example.com", "fixture");
    });
    await page.evaluate(async () =>
      (await import("/src/lib/api.ts")).api.toggleLike("fixture"),
    );
    await page.goto(`${origin}/me/uploads`);
    await page.getByText("A private upload", { exact: true }).waitFor();
    await page.evaluate(async () => {
      const { api } = await import("/src/lib/api.ts");
      // Simulate a transport that cannot cancel: stale responses still need a guard.
      const original = window.fetch;
      window.fetch = (url, init) =>
        original(url, { ...init, signal: undefined });
      window.pendingOld = [
        api.getMyUploads({ offset: 999 }),
        api.getNotifications({ offset: 999 }),
      ].map((p) =>
        p.then(
          () => "returned",
          (e) => e.name,
        ),
      );
    });
    await page.waitForFunction(() => true);
    const deadline = Date.now() + 10000;
    while (held.length !== 2 && Date.now() < deadline)
      await new Promise((resolve) => setTimeout(resolve, 20));
    assert.equal(held.length, 2);
    await page.evaluate(async () => {
      const { useAuthStore } = await import("/src/store/useAuthStore.ts");
      window.leaked = false;
      const observer = new MutationObserver(() => {
        if (
          useAuthStore.getState().user?.id === "b" &&
          document.body.textContent.includes("A private upload")
        )
          window.leaked = true;
      });
      observer.observe(document.body, {
        childList: true,
        subtree: true,
        characterData: true,
      });
      useAuthStore.getState().logout();
      await useAuthStore.getState().login("b@example.com", "fixture");
    });
    await page.getByText("B private upload", { exact: true }).waitFor();
    await held[0].fulfill({ json: { items: [upload("a")], total: 1 } });
    await held[1].fulfill({ status: 401, json: { error: "A expired" } });
    const guarded = await page.evaluate(async () => {
      const { useAuthStore } = await import("/src/store/useAuthStore.ts");
      return {
        results: await Promise.all(window.pendingOld),
        account: useAuthStore.getState().user.id,
        token: sessionStorage.getItem("ttl_user_token"),
        leaked: window.leaked,
      };
    });
    assert.deepEqual(guarded, {
      results: ["AbortError", "AbortError"],
      account: "b",
      token: "b-token",
      leaked: false,
    });
    assert.equal(
      await page.getByText("A private upload", { exact: true }).count(),
      0,
    );
    await page.evaluate(async () =>
      (await import("/src/lib/api.ts")).api.toggleLike("fixture"),
    );
    const admin = await page.evaluate(async () => {
      const { api } = await import("/src/lib/api.ts");
      const { useAuthStore } = await import("/src/store/useAuthStore.ts");
      sessionStorage.setItem("ttl_admin_token", "expired-admin");
      await api.adminGetStats().catch(() => {});
      return {
        account: useAuthStore.getState().user.id,
        admin: sessionStorage.getItem("ttl_admin_token"),
      };
    });
    assert.deepEqual(admin, { account: "b", admin: null });
    await page.evaluate(async () => {
      const { useAuthStore } = await import("/src/store/useAuthStore.ts");
      window.pendingLogin = useAuthStore
        .getState()
        .login("slow@example.com", "fixture")
        .then(
          () => "returned",
          (e) => e.name,
        );
    });
    await waitingLogin;
    await page.evaluate(async () =>
      (await import("/src/store/useAuthStore.ts")).useAuthStore
        .getState()
        .logout(),
    );
    releaseLogin();
    const late = await page.evaluate(async () => {
      const { useAuthStore } = await import("/src/store/useAuthStore.ts");
      return {
        result: await window.pendingLogin,
        user: useAuthStore.getState().user,
        token: sessionStorage.getItem("ttl_user_token"),
        saved: sessionStorage.getItem("ttl_user_account"),
      };
    });
    assert.deepEqual(late, {
      result: "AbortError",
      user: null,
      token: null,
      saved: null,
    });
    await page.evaluate(async () =>
      (await import("/src/lib/api.ts")).api.toggleLike("fixture"),
    );
    assert.deepEqual(likeCredentials, [
      "Bearer a-token",
      "Bearer b-token",
      null,
    ]);
    await context.close();
    console.log(
      "PASS: account UI/cache reset, uncancellable old success/401, separate admin expiry, and logout during login",
    );
  } finally {
    await browser.close();
    await server.close();
    process.chdir(previous);
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
