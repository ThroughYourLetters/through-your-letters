import { defineConfig, loadEnv, type ProxyOptions } from "vite";
import type { ClientRequest, IncomingMessage } from "node:http";
import react from "@vitejs/plugin-react";
import { VitePWA } from "vite-plugin-pwa";
import path from "path";
import limits from "../api/config/runtime.json";

const setClientHeaders: NonNullable<ProxyOptions["configure"]> = (proxy) => {
  const overwrite = (outgoing: ClientRequest, incoming: IncomingMessage) => {
    outgoing.removeHeader("x-forwarded-for");
    outgoing.removeHeader("x-real-ip");
    outgoing.removeHeader("forwarded");
    if (incoming.socket.remoteAddress) {
      outgoing.setHeader("x-forwarded-for", incoming.socket.remoteAddress);
    }
  };
  proxy.on("proxyReq", overwrite);
  proxy.on("proxyReqWs", overwrite);
};

export default defineConfig(({ mode, command }) => {
  // Where the dev server forwards /api and /ws. Defaults to a local API;
  // `pnpm dev:live` points it at the deployed backend instead.
  const env = loadEnv(mode, process.cwd(), "");
  const apiTarget = env.API_PROXY_TARGET || "http://localhost:3000";
  const offlineBackend =
    command === "serve" && env.VITE_API_URL === "same-origin"
      ? apiTarget
      : env.VITE_API_URL || "http://localhost:3000";

  return {
    define: {
      "import.meta.env.VITE_OFFLINE_BACKEND": JSON.stringify(offlineBackend),
    },
    plugins: [
      react(),
      VitePWA({
        registerType: "autoUpdate",
        manifest: {
          name: "Through Your Letters",
          short_name: "TYL",
          description:
            "A collaborative street typography archive for Indian cities",
          theme_color: "#cc543a",
          background_color: "#ffffff",
          display: "standalone",
          start_url: "/",
          icons: [
            { src: "/icon-192.svg", sizes: "any", type: "image/svg+xml" },
            {
              src: "/icon-512.svg",
              sizes: "any",
              type: "image/svg+xml",
              purpose: "maskable",
            },
          ],
        },
        workbox: {
          // API data stays network-only; React Query owns short-lived UI state.
          importScripts: ["/cache-policy.js"],
          globPatterns: ["**/*.{js,css,html,ico,png,svg,woff2}"],
          runtimeCaching: [
            {
              urlPattern: /^https:\/\/.*\.r2\.dev\/.*/i,
              handler: "CacheFirst",
              options: {
                cacheName: "r2-images",
                expiration: {
                  maxEntries: limits.web.image_cache_entries,
                  maxAgeSeconds: limits.web.image_cache_seconds,
                },
                cacheableResponse: { statuses: [0, 200] },
              },
            },
          ],
        },
      }),
    ],
    server: {
      port: mode === "live" ? limits.web.live_dev_port : limits.web.dev_port,
      strictPort: true,
      host: "0.0.0.0",
      proxy: {
        "/api": {
          target: apiTarget,
          changeOrigin: true,
          configure: setClientHeaders,
        },
        "/ws": {
          target: apiTarget,
          changeOrigin: true,
          ws: true,
          configure: setClientHeaders,
        },
      },
    },
    resolve: {
      alias: {
        "@": path.resolve(__dirname, "./src"),
      },
    },
    build: {
      outDir: "dist",
      sourcemap: true,
    },
  };
});
