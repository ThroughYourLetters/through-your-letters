export interface MapMarker {
  id: string;
  lat: number;
  lng: number;
  thumbnail: string;
}

export function groupMarkers<T extends { lat: number; lng: number }>(
  markers: T[],
): T[][] {
  const groups = new Map<string, T[]>();
  for (const marker of markers) {
    if (
      !Number.isFinite(marker.lat) ||
      !Number.isFinite(marker.lng) ||
      Math.abs(marker.lat) > 90 ||
      Math.abs(marker.lng) > 180
    )
      continue;
    // Match coordinates despite floating point noise.
    const key = `${Math.round(marker.lat * 1_000_000)},${Math.round(marker.lng * 1_000_000)}`;
    const group = groups.get(key);
    if (group) group.push(marker);
    else groups.set(key, [marker]);
  }
  return [...groups.values()];
}
