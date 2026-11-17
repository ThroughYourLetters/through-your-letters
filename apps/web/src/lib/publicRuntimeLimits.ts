import type bundled from "../../../api/config/runtime.json";

type Rule = readonly [number, number];
type Rules<T> = Record<keyof T, Rule>;

function object(value: unknown, name: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${name} must be an object`);
  }
  return value as Record<string, unknown>;
}

function numbers<T extends Record<string, Rule>>(
  input: unknown,
  name: string,
  rules: T,
): { [K in keyof T]: number } {
  const source = object(input, name);
  return Object.fromEntries(
    Object.entries(rules).map(([key, [min, max]]) => {
      const value = source[key];
      if (
        typeof value !== "number" ||
        !Number.isSafeInteger(value) ||
        value < min ||
        value > max
      ) {
        throw new Error(
          `${name}.${key} must be an integer from ${min} to ${max}`,
        );
      }
      return [key, value];
    }),
  ) as { [K in keyof T]: number };
}

const webRules = {
  request_timeout_ms: [1000, 120000],
  search_debounce_ms: [100, 2000],
  city_search_debounce_ms: [100, 2000],
  saved_upload_refresh_ms: [500, 60000],
  offline_max_entries: [1, 1000],
  offline_max_bytes: [1024, 1_073_741_824],
  offline_page_size: [1, 1000],
  offline_retry_delay_seconds: [1, 3600],
  offline_lease_margin_ms: [1000, 60_000],
  geolocation_timeout_seconds: [1, 60],
  dev_port: [1, 65535],
  live_dev_port: [1, 65535],
  gallery_page_size: [1, 1000],
  query_stale_seconds: [0, 86400],
  query_retries: [0, 5],
  image_cache_entries: [1, 10000],
  image_cache_seconds: [1, 31536000],
} satisfies Rules<typeof bundled.web>;

const mapRules = {
  nearby_page_size: [1, 20000],
  nearby_max_limit: [1, 20000],
  max_nearby_radius_m: [1, 20_100_000],
  max_cells: [32, 4096],
  grid_pixels: [8, 256],
  query_timeout_ms: [100, 30000],
  move_debounce_ms: [100, 2000],
  query_gc_seconds: [1, 60],
  heat_radius_pixels: [8, 128],
  heat_blur_pixels: [1, 128],
  heat_saturation_count: [2, 1_000_000],
  detail_zoom: [10, 20],
} satisfies Rules<typeof bundled.map>;

const pageCeilings = {
  city_discovery: 50,
  capitals: 500,
  owner: 100,
  search: 100,
  discovery: 100,
  collection_items: 200,
  moderation: 200,
  cities: 500,
  regions: 500,
  activity: 100,
  events: 100,
  jobs: 100,
  markers: 10000,
  coverage: 20000,
} satisfies Record<keyof typeof bundled.pages, number>;

function page(input: unknown, name: string, ceiling: number) {
  const result = numbers(input, name, {
    default_limit: [1, ceiling],
    max_limit: [1, ceiling],
  });
  if (result.default_limit > result.max_limit) {
    throw new Error(`${name}.default_limit exceeds max_limit`);
  }
  return result;
}

/** Validate browser budgets at build time and exclude server configuration. */
export function publicRuntimeLimits(input: unknown) {
  const source = object(input, "runtime configuration");
  const web = numbers(source.web, "web", webRules);
  const cityServices = numbers(source.city_services, "city_services", {
    total_timeout_ms: [100, 60000],
  });
  if (web.request_timeout_ms < cityServices.total_timeout_ms + 1000)
    throw new Error(
      "web.request_timeout_ms must exceed the city import deadline by one second",
    );
  const map = numbers(source.map, "map", mapRules);
  const gallery = page(source.gallery, "gallery", 1000);
  const rawPages = object(source.pages, "pages");
  const pages = Object.fromEntries(
    Object.entries(pageCeilings).map(([key, ceiling]) => [
      key,
      page(rawPages[key], `pages.${key}`, ceiling),
    ]),
  ) as { [K in keyof typeof pageCeilings]: typeof gallery };
  const images = numbers(source.images, "images", {
    upload_timeout_ms: [100, 180000],
    max_input_bytes: [1024, 33_554_432],
  });
  if (web.dev_port === web.live_dev_port)
    throw new Error("web development ports must differ");
  if (
    web.offline_page_size > web.offline_max_entries ||
    web.offline_max_bytes < images.max_input_bytes
  ) {
    throw new Error("web offline page or byte budget exceeds capacity");
  }
  if (web.gallery_page_size > gallery.max_limit)
    throw new Error("web gallery page exceeds gallery.max_limit");
  if (map.nearby_page_size > map.nearby_max_limit)
    throw new Error("map nearby page exceeds nearby_max_limit");
  if ((map.grid_pixels & (map.grid_pixels - 1)) !== 0)
    throw new Error("map.grid_pixels must be a power of two");
  return { web, map, gallery, pages, images };
}

export type PublicRuntimeLimits = ReturnType<typeof publicRuntimeLimits>;
