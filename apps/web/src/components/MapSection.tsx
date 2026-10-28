import React, { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
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

const MapSection: React.FC = () => {
  const {
    selectedCityId,
    selectedCityName,
    centerLat,
    centerLng,
    defaultZoom,
  } = useCityStore();
  const {
    data: coverage = [],
    isLoading: loading,
    isError,
    refetch,
  } = useQuery<CoveragePoint[]>({
    queryKey: ["map", "coverage", selectedCityId],
    queryFn: ({ signal }) =>
      api.getCoverage({ cityId: selectedCityId, limit: 5000, signal }),
  });

  const topCoverage = useMemo(() => coverage.slice(0, 90), [coverage]);
  const mapCenter: [number, number] = selectedCityId
    ? [centerLat, centerLng]
    : [20, 0];
  const mapZoom = selectedCityId ? Math.max(defaultZoom, 5) : 2;

  return (
    <div className="space-y-16 animate-in">
      <div className="border-b border-line pb-8 space-y-4">
        <h2 className="font-display font-semibold text-5xl md:text-7xl tracking-tight leading-none">
          Map
        </h2>
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
          <p className="text-xs font-semibold uppercase text-stone-400 max-w-xl">
            Coverage is calculated from approved uploads{" "}
            {selectedCityId ? `in ${selectedCityName}` : "across every city"}.
          </p>
        </div>
      </div>

      <LeafletMap center={mapCenter} zoom={mapZoom} />

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-12">
        <div className="space-y-8">
          <section className="border border-line p-6 space-y-3">
            <h4 className="text-xs font-semibold uppercase tracking-widest text-stone-400">
              Legend
            </h4>
            <div className="space-y-2">
              {[
                { label: "0–2 uploads", color: "bg-[#cc543a]/10" },
                { label: "3–5 uploads", color: "bg-[#cc543a]/25" },
                { label: "6–10 uploads", color: "bg-[#cc543a]/50" },
                { label: "11–20 uploads", color: "bg-[#cc543a]/75" },
                { label: "21+ uploads", color: "bg-[#cc543a]" },
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
        </div>

        <div className="lg:col-span-2 grid grid-cols-2 md:grid-cols-3 gap-6 bg-surface border border-line p-8 shadow-card">
          {loading ? (
            <div className="col-span-full flex justify-center py-20">
              <Loader2 className="animate-spin text-terracotta" size={32} />
            </div>
          ) : isError ? (
            <div role="alert" className="col-span-full text-center py-8">
              Could not load coverage.{" "}
              <button className="underline" onClick={() => void refetch()}>
                Retry coverage
              </button>
            </div>
          ) : topCoverage.length === 0 ? (
            <div className="col-span-full text-center py-20 text-stone-400 font-semibold uppercase">
              No coverage data yet
            </div>
          ) : (
            topCoverage.map((point) => {
              const heatColor = getHeatColor(point.count);
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
                    {point.count === 1 ? "upload" : "uploads"}
                  </p>
                </div>
              );
            })
          )}
        </div>
      </div>

      {!loading && !isError && coverage.length > 0 && (
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
