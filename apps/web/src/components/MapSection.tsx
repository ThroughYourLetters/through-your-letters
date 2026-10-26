import React, { useEffect, useMemo, useState } from "react";
import { Target, Info, Globe, Loader2 } from "lucide-react";
import LeafletMap from "./LeafletMap";
import { api } from "../lib/api";
import { useCityStore } from "../store/useCityStore";

interface CoveragePoint {
  pin_code: string;
  city_id: string;
  city_name: string;
  lat: number;
  lng: number;
  count: number;
}

function getHeatColor(count: number): string {
  if (count <= 2) return "bg-[#cc543a]/10 text-[#cc543a]/70";
  if (count <= 5) return "bg-[#cc543a]/25 text-[#cc543a]/80";
  if (count <= 10) return "bg-[#cc543a]/50 text-[#cc543a]";
  if (count <= 20) return "bg-[#cc543a]/75 text-white";
  return "bg-[#cc543a] text-white";
}

function getHeatLabel(count: number): string {
  if (count <= 2) return "Desert";
  if (count <= 5) return "Sparse";
  if (count <= 10) return "Growing";
  if (count <= 20) return "Active";
  return "Oasis";
}

const MapSection: React.FC = () => {
  const {
    selectedCityId,
    selectedCityName,
    centerLat,
    centerLng,
    defaultZoom,
  } = useCityStore();
  const [coverage, setCoverage] = useState<CoveragePoint[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    api
      .getCoverage({ cityId: selectedCityId, limit: 5000 })
      .then((data) => setCoverage(data))
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [selectedCityId]);

  const topCoverage = useMemo(() => coverage.slice(0, 90), [coverage]);
  const mapCenter: [number, number] = selectedCityId
    ? [centerLat, centerLng]
    : [20, 0];
  const mapZoom = selectedCityId ? Math.max(defaultZoom, 5) : 2;

  return (
    <div className="space-y-16 animate-in">
      <div className="border-b border-line pb-8 space-y-4">
        <h2 className="font-display font-semibold text-5xl md:text-7xl tracking-tight leading-none">
          The <span className="text-terracotta">Archive Heatmap</span>
        </h2>
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
          <p className="text-xs font-semibold uppercase text-stone-400 max-w-xl">
            Coverage is calculated from approved uploads{" "}
            {selectedCityId ? `in ${selectedCityName}` : "across every city"}.
          </p>
          <div className="bg-ink text-white px-3 py-1 text-xs font-semibold uppercase tracking-widest flex items-center gap-2">
            <Target size={12} className="text-ochre" /> Global target: 10+
            artifacts per locality
          </div>
        </div>
      </div>

      <LeafletMap center={mapCenter} zoom={mapZoom} />

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-12">
        <div className="space-y-8">
          <section className="bg-ink text-white p-8 shadow-soft space-y-6">
            <h3 className="text-sm font-semibold uppercase flex items-center gap-2 border-b border-white/20 pb-4">
              <Info size={16} className="text-terracotta" /> Purpose
            </h3>
            <p className="text-xs leading-relaxed text-stone-300 font-medium italic">
              This view highlights low-coverage localities first so contributors
              can document typographic deserts.
            </p>
          </section>

          <section className="border border-line p-6 space-y-3">
            <h4 className="text-xs font-semibold uppercase tracking-widest text-stone-400">
              Legend
            </h4>
            <div className="space-y-2">
              {[
                { label: "Desert (<=2)", color: "bg-[#cc543a]/10" },
                { label: "Sparse (3-5)", color: "bg-[#cc543a]/25" },
                { label: "Growing (6-10)", color: "bg-[#cc543a]/50" },
                { label: "Active (11-20)", color: "bg-[#cc543a]/75" },
                { label: "Oasis (20+)", color: "bg-[#cc543a]" },
              ].map((l) => (
                <div key={l.label} className="flex items-center gap-3">
                  <div
                    className={`w-5 h-5 border border-line ${l.color}`}
                  ></div>
                  <span className="text-xs font-semibold uppercase">
                    {l.label}
                  </span>
                </div>
              ))}
            </div>
          </section>

          <section className="bg-stone-100 border border-dashed border-line p-6 space-y-4">
            <Globe size={32} className="opacity-20" />
            <p className="text-xs font-bold text-stone-500">
              City metadata and coverage are fetched dynamically, so newly
              discovered cities appear without manual code updates.
            </p>
          </section>
        </div>

        <div className="lg:col-span-2 grid grid-cols-2 md:grid-cols-3 gap-6 bg-surface border border-line p-8 shadow-card">
          {loading ? (
            <div className="col-span-full flex justify-center py-20">
              <Loader2 className="animate-spin text-terracotta" size={32} />
            </div>
          ) : topCoverage.length === 0 ? (
            <div className="col-span-full text-center py-20 text-stone-400 font-semibold uppercase">
              No coverage data yet
            </div>
          ) : (
            topCoverage.map((point) => {
              const heatColor = getHeatColor(point.count);
              const heatLabel = getHeatLabel(point.count);
              const locality = `${point.city_name} ${point.pin_code}`;

              return (
                <div
                  key={`${point.city_id}-${point.pin_code}`}
                  className={`aspect-square border border-line ${heatColor} flex flex-col items-center justify-center text-center p-4 transition-colors relative group`}
                >
                  <span className="font-display font-semibold text-4xl mb-1">
                    {point.count}
                  </span>
                  <p className="text-xs font-semibold uppercase tracking-tight line-clamp-2">
                    {locality}
                  </p>
                  <p className="text-xs font-bold uppercase tracking-widest mt-1 opacity-70">
                    {heatLabel}
                  </p>
                </div>
              );
            })
          )}
        </div>
      </div>

      {!loading && coverage.length > 0 && (
        <div className="border border-line p-8 bg-surface shadow-soft space-y-6">
          <h3 className="font-display font-semibold tracking-tight text-xl">
            Global Coverage Index
          </h3>
          <div className="flex flex-wrap gap-3">
            {coverage.map((point) => (
              <div
                key={`index-${point.city_id}-${point.pin_code}`}
                className="bg-stone-50 border border-line px-4 py-2 flex items-center gap-3"
              >
                <span className="text-xs font-semibold">
                  {point.city_name} - {point.pin_code}
                </span>
                <span className="text-xs font-semibold text-terracotta">
                  {point.count}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};

export default MapSection;
