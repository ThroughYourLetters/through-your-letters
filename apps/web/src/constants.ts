const configuredApiUrl = import.meta.env.VITE_API_URL;

// "same-origin" sends API calls to the page's own host, where the dev
// server's proxy forwards them on (see vite.config.ts)
export const API_BASE_URL =
  configuredApiUrl === "same-origin"
    ? window.location.origin
    : configuredApiUrl || "http://localhost:3000";

