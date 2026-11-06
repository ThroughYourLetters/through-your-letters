import config from "../../../api/config/runtime.json";

export const webLimits = config.web;

export const mapLimits = config.map;

export const uploadTimeoutMs = config.images.upload_timeout_ms;
