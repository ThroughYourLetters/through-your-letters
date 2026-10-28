export function offlineDatabaseName(
  backend: string,
  mode: string,
  origin: string,
): string {
  const url = new URL(backend === "same-origin" ? origin : backend);
  url.username = "";
  url.password = "";
  url.search = "";
  url.hash = "";
  const identity = [mode, url.origin, url.pathname.replace(/\/+$/, "")];
  return `tyl-offline-v2:${encodeURIComponent(JSON.stringify(identity))}`;
}
