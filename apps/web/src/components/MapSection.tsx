import { useSearchParams } from "react-router-dom";
import LeafletMap from "./LeafletMap";
import { ScriptPills } from "./FilterBar";
import { STYLE_OPTIONS } from "../types";
import { useCityStore } from "../store/useCityStore";

export default function MapSection() {
  const { selectedCityId, centerLat, centerLng, defaultZoom } = useCityStore();
  const [params, setParams] = useSearchParams();
  return (
    <div className="space-y-4 animate-in">
      <header className="flex items-center justify-between gap-3">
        <h2 className="font-display font-semibold text-2xl md:text-3xl">Map</h2>
        <label>
          <span className="sr-only">Style</span>
          <select
            aria-label="Style"
            className="rounded-full border border-line bg-surface px-3 py-2 text-sm"
            value={params.get("style") || ""}
            onChange={(event) => {
              const next = new URLSearchParams(params);
              if (event.target.value) next.set("style", event.target.value);
              else next.delete("style");
              setParams(next, { replace: true });
            }}
          >
            <option value="">All styles</option>
            {STYLE_OPTIONS.map((style) => (
              <option key={style} value={style}>
                {style}
              </option>
            ))}
          </select>
        </label>
      </header>
      <ScriptPills />
      <LeafletMap
        key={selectedCityId ?? "all"}
        center={selectedCityId ? [centerLat, centerLng] : [20, 0]}
        zoom={selectedCityId ? Math.max(defaultZoom, 5) : 2}
      />
      <p className="text-xs text-muted">
        Tap hotspots for photos. Locations are approximate postal areas.
      </p>
    </div>
  );
}
