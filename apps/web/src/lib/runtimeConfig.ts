import type { PublicRuntimeLimits } from "./publicRuntimeLimits";

declare const __TTL_RUNTIME_LIMITS__: PublicRuntimeLimits;
const config = __TTL_RUNTIME_LIMITS__;

export const webLimits = config.web;
export const mapLimits = config.map;
export const pageLimits = config.pages;
export const galleryLimits = config.gallery;
export const uploadTimeoutMs = config.images.upload_timeout_ms;

export const moderationPageSizes = [
  ...new Set([
    Math.min(25, pageLimits.moderation.max_limit),
    ...[10, 25, 50, 100].filter(
      (size) => size <= pageLimits.moderation.max_limit,
    ),
  ]),
].sort((a, b) => a - b);
