import React, { useEffect, useMemo, useState } from "react";
import { Target, Info, Globe, Loader2 } from "lucide-react";
import { PIN_AREA_MAP } from "../constants";
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
      <div className="border-b-4 border-black pb-8 space-y-4">
        <h2 className="text-5xl md:text-7xl font-black uppercase tracking-tighter leading-none">
          The <span className="text-[#cc543a]">Archive Heatmap</span>
        </h2>
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
          <p className="text-xs font-black uppercase text-slate-400 max-w-xl">
            Coverage is calculated from approved uploads{" "}
            {selectedCityId ? `in ${selectedCityName}` : "across every city"}.
          </p>
          <div className="bg-black text-white px-3 py-1 text-[9px] font-black uppercase tracking-widest flex items-center gap-2">
            <Target size={12} className="text-[#d4a017]" /> Global target: 10+
            artifacts per locality
          </div>
        </div>
      </div>

      <LeafletMap center={mapCenter} zoom={mapZoom} />

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-12">
        <div className="space-y-8">
          <section className="bg-black text-white p-8 brutalist-shadow-sm space-y-6">
            <h3 className="text-sm font-black uppercase flex items-center gap-2 border-b border-white/20 pb-4">
              <Info size={16} className="text-[#cc543a]" /> Purpose
            </h3>
            <p className="text-xs leading-relaxed text-slate-300 font-medium italic">
              This view highlights low-coverage localities first so contributors
              can document typographic deserts.
            </p>
          </section>

          <section className="border-2 border-black p-6 space-y-3">
            <h4 className="text-[10px] font-black uppercase tracking-widest text-slate-400">
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
                    className={`w-5 h-5 border border-black/10 ${l.color}`}
                  ></div>
                  <span className="text-[9px] font-black uppercase">
                    {l.label}
                  </span>
                </div>
              ))}
            </div>
          </section>

          <section className="bg-slate-100 border-2 border-dashed border-black/20 p-6 space-y-4">
            <Globe size={32} className="opacity-20" />
            <p className="text-[10px] font-bold text-slate-500">
              City metadata and coverage are fetched dynamically, so newly
              discovered cities appear without manual code updates.
            </p>
          </section>
        </div>

        <div className="lg:col-span-2 grid grid-cols-2 md:grid-cols-3 gap-6 bg-white border-4 border-black p-8 brutalist-shadow">
          {loading ? (
            <div className="col-span-full flex justify-center py-20">
              <Loader2 className="animate-spin text-[#cc543a]" size={32} />
            </div>
          ) : topCoverage.length === 0 ? (
            <div className="col-span-full text-center py-20 text-slate-400 font-black uppercase">
              No coverage data yet
            </div>
          ) : (
            topCoverage.map((point) => {
              const heatColor = getHeatColor(point.count);
              const heatLabel = getHeatLabel(point.count);
              const locality =
                PIN_AREA_MAP[point.pin_code] ||
                `${point.city_name} ${point.pin_code}`;

              return (
                <div
                  key={`${point.city_id}-${point.pin_code}`}
                  className={`aspect-square border-2 border-black ${heatColor} flex flex-col items-center justify-center text-center p-4 transition-colors relative group`}
                >
                  <span className="text-4xl font-black mb-1">
                    {point.count}
                  </span>
                  <p className="text-[9px] font-black uppercase tracking-tighter line-clamp-2">
                    {locality}
                  </p>
                  <p className="text-[7px] font-bold uppercase tracking-widest mt-1 opacity-70">
                    {heatLabel}
                  </p>
                </div>
              );
            })
          )}
        </div>
      </div>

      {!loading && coverage.length > 0 && (
        <div className="border-4 border-black p-8 bg-white brutalist-shadow-sm space-y-6">
          <h3 className="text-xl font-black uppercase tracking-tighter">
            Global Coverage Index
          </h3>
          <div className="flex flex-wrap gap-3">
            {coverage.map((point) => (
              <div
                key={`index-${point.city_id}-${point.pin_code}`}
                className="bg-slate-50 border-2 border-black px-4 py-2 flex items-center gap-3"
              >
                <span className="text-[10px] font-black">
                  {point.city_name} - {point.pin_code}
                </span>
                <span className="text-[10px] font-black text-[#cc543a]">
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
