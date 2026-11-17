import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { resolveConfig } from "vite";
import { publicRuntimeLimits } from "../apps/web/src/lib/publicRuntimeLimits.ts";

const root = fileURLToPath(new URL("../apps/web/", import.meta.url));
const bundled = JSON.parse(
  readFileSync(
    new URL("../apps/api/config/runtime.json", import.meta.url),
    "utf8",
  ),
);
const defaults = publicRuntimeLimits(bundled);
assert.equal(defaults.web.gallery_page_size, 30);
assert.equal(defaults.pages.owner.default_limit, 20);
assert.equal("database" in defaults, false);
let cases = 0;
for (const [section, key, value] of [
  ["web", "request_timeout_ms", 999],
  ["web", "request_timeout_ms", 30000],
  ["web", "search_debounce_ms", 99],
  ["web", "saved_upload_refresh_ms", 499],
  ["web", "query_retries", 6],
  ["web", "query_retries", -1],
  ["web", "query_stale_seconds", 86401],
  ["web", "query_stale_seconds", "300"],
  ["web", "image_cache_entries", 10001],
  ["web", "image_cache_seconds", 0],
  ["web", "offline_max_bytes", 1024],
  ["web", "offline_page_size", 1001],
  ["web", "dev_port", bundled.web.live_dev_port],
  ["web", "gallery_page_size", 101],
  ["map", "grid_pixels", 63],
  ["map", "nearby_page_size", 20001],
  ["map", "move_debounce_ms", 1.5],
  ["images", "upload_timeout_ms", Infinity],
]) {
  const input = structuredClone(bundled);
  input[section][key] = value;
  assert.throws(
    () => publicRuntimeLimits(input),
    undefined,
    `${section}.${key}`,
  );
  cases++;
}
for (const [name, policy] of Object.entries(bundled.pages)) {
  for (const [key, value] of [
    ["default_limit", 0],
    ["default_limit", policy.max_limit + 1],
    ["max_limit", 20001],
  ]) {
    const input = structuredClone(bundled);
    input.pages[name][key] = value;
    assert.throws(
      () => publicRuntimeLimits(input),
      undefined,
      `pages.${name}.${key}`,
    );
    cases++;
  }
}
const directory = mkdtempSync(path.join(tmpdir(), "ttl-runtime-"));
const previous = process.env.TTL_LIMITS_FILE;
try {
  const override = structuredClone(bundled);
  override.web.gallery_page_size = 7;
  override.pages.owner.default_limit = 3;
  override.auth.private_canary = "must-not-enter-the-browser";
  const filename = path.join(directory, "limits.json");
  writeFileSync(filename, JSON.stringify(override));
  process.env.TTL_LIMITS_FILE = filename;
  const config = await resolveConfig(
    { root, configFile: path.join(root, "vite.config.ts"), logLevel: "silent" },
    "build",
  );
  const exposed = JSON.parse(config.define.__TTL_RUNTIME_LIMITS__);
  assert.equal(exposed.web.gallery_page_size, 7);
  assert.equal(exposed.pages.owner.default_limit, 3);
  assert.equal(
    JSON.stringify(exposed).includes("must-not-enter-the-browser"),
    false,
  );
  override.web.query_retries = 6;
  writeFileSync(filename, JSON.stringify(override));
  await assert.rejects(
    resolveConfig(
      {
        root,
        configFile: path.join(root, "vite.config.ts"),
        logLevel: "silent",
      },
      "build",
    ),
    /web.query_retries/,
  );
  process.env.TTL_LIMITS_FILE = "";
  await assert.rejects(
    resolveConfig(
      {
        root,
        configFile: path.join(root, "vite.config.ts"),
        logLevel: "silent",
      },
      "build",
    ),
    /TTL_LIMITS_FILE/,
  );
} finally {
  if (previous === undefined) delete process.env.TTL_LIMITS_FILE;
  else process.env.TTL_LIMITS_FILE = previous;
  rmSync(directory, { recursive: true, force: true });
}
console.log(`${cases} invalid public budgets and actual Vite overrides passed`);
