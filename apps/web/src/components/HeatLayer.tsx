import { useEffect } from "react";
import { useMap } from "react-leaflet";
import L from "leaflet";
import simpleheat from "simpleheat";
import { mapLimits } from "../lib/runtimeConfig";
import type { DensityCell } from "../types/mapDensity";

export function nearestLongitude(lng: number, center: number): number {
  return lng + 360 * Math.round((center - lng) / 360);
}

class DensityLayer extends L.Layer {
  private readonly canvas = document.createElement("canvas");
  private readonly heat = simpleheat(this.canvas);
  private attachedMap: L.Map | undefined;
  private frame: number | undefined;

  constructor(private readonly cells: DensityCell[]) {
    super();
    this.canvas.className = "leaflet-layer leaflet-zoom-hide map-density-layer";
    this.heat
      .radius(mapLimits.heat_radius_pixels, mapLimits.heat_blur_pixels)
      .max(
        Math.max(
          mapLimits.heat_saturation_count,
          ...cells.map((cell) => cell.count),
        ),
      )
      .gradient({
        0.15: "#164dff",
        0.35: "#00cfff",
        0.55: "#00de73",
        0.75: "#ffe600",
        1: "#ff2100",
      });
  }

  onAdd(map: L.Map): this {
    this.attachedMap = map;
    const pane = map.getPane("overlayPane");
    if (!pane) throw new Error("Map overlay pane is missing");
    pane.appendChild(this.canvas);
    map.on("moveend resize zoomend", this.schedule);
    this.draw();
    return this;
  }

  onRemove(map: L.Map): this {
    map.off("moveend resize zoomend", this.schedule);
    // Prevent redraws against a removed map.
    if (this.frame !== undefined) cancelAnimationFrame(this.frame);
    this.frame = undefined;
    this.attachedMap = undefined;
    this.canvas.remove();
    return this;
  }

  private readonly schedule = () => {
    if (this.frame !== undefined || !this.attachedMap) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = undefined;
      this.draw();
    });
  };

  private draw() {
    const map = this.attachedMap;
    if (!map) return;
    const size = map.getSize();
    if (size.x <= 0 || size.y <= 0) return;
    const bounds = map.getBounds();
    L.DomUtil.setPosition(this.canvas, map.containerPointToLayerPoint([0, 0]));
    this.canvas.width = size.x;
    this.canvas.height = size.y;
    this.heat.resize();
    const points: Array<[number, number, number]> = [];
    const padding = mapLimits.heat_radius_pixels + mapLimits.heat_blur_pixels;
    // Paint every visible copy across datelines.
    for (const cell of this.cells) {
      const first = Math.floor((bounds.getWest() - cell.lng) / 360);
      const last = Math.ceil((bounds.getEast() - cell.lng) / 360);
      for (let copy = first; copy <= last; copy++) {
        const point = map.latLngToContainerPoint([
          cell.lat,
          cell.lng + 360 * copy,
        ]);
        if (
          point.x >= -padding &&
          point.x <= size.x + padding &&
          point.y >= -padding &&
          point.y <= size.y + padding
        )
          points.push([point.x, point.y, cell.count]);
      }
    }
    this.heat.data(points).draw(0.2);
  }
}

export default function HeatLayer({ cells }: { cells: DensityCell[] }) {
  const map = useMap();
  useEffect(() => {
    const layer = new DensityLayer(cells).addTo(map);
    return () => {
      map.removeLayer(layer);
    };
  }, [map, cells]);
  return null;
}
