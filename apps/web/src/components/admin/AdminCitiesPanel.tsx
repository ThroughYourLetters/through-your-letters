import React, { useCallback, useEffect, useState } from "react";
import { Globe, Loader2, RefreshCw, Rocket, Search } from "lucide-react";
import { api } from "../../lib/api";
import { useToastStore } from "../../store/useToastStore";

interface CityItem {
  id: string;
  name: string;
  country_code: string;
  center_lat: number | null;
  center_lng: number | null;
  default_zoom: number | null;
  description?: string | null;
  cover_image_url?: string | null;
  is_active: boolean | null;
}

const AdminCitiesPanel: React.FC = () => {
  const { addToast } = useToastStore();

  const [query, setQuery] = useState("");
  const [countryCode, setCountryCode] = useState("");
  const [limit, setLimit] = useState(100);

  const [cities, setCities] = useState<CityItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [lastSync, setLastSync] = useState<{
    processed: number;
    upserted: number;
    failed: number;
  } | null>(null);

  const loadCities = useCallback(async () => {
    setLoading(true);
    try {
      const data = await api.getCities({
        q: query.trim() || undefined,
        countryCode: countryCode.trim() || undefined,
        limit: Math.min(Math.max(limit, 1), 500),
        offset: 0,
      });
      setCities(data);
    } catch (e) {
      addToast((e as Error).message || "Failed to load cities", "error");
    } finally {
      setLoading(false);
    }
  }, [addToast, countryCode, limit, query]);

  useEffect(() => {
    void loadCities();
  }, [loadCities]);

  const handleDiscover = async () => {
    const q = query.trim();
    if (q.length < 2) {
      addToast("Enter at least 2 characters to discover cities", "warning");
      return;
    }

    setSyncing(true);
    try {
      const result = await api.adminDiscoverCities({
        query: q,
        country_code: countryCode.trim() || undefined,
        limit: Math.min(Math.max(limit, 1), 100),
      });
      setLastSync(result);
      addToast(
        `Discovery complete: ${result.upserted} upserted, ${result.failed} failed`,
        result.failed > 0 ? "warning" : "success",
      );
      await loadCities();
    } catch (e) {
      addToast((e as Error).message || "City discovery failed", "error");
    } finally {
      setSyncing(false);
    }
  };

  const handleBootstrapCapitals = async () => {
    if (
      !window.confirm(
        "Bootstrap global capitals now? This may take a while and call external APIs.",
      )
    ) {
      return;
    }

    setSyncing(true);
    try {
      const result = await api.adminBootstrapCapitals({
        limit: Math.min(Math.max(limit, 1), 500),
      });
      setLastSync(result);
      addToast(
        `Capital bootstrap complete: ${result.upserted} upserted, ${result.failed} failed`,
        result.failed > 0 ? "warning" : "success",
      );
      await loadCities();
    } catch (e) {
      addToast((e as Error).message || "Capital bootstrap failed", "error");
    } finally {
      setSyncing(false);
    }
  };

  return (
    <div className="space-y-8">
      <div className="bg-white border-4 border-black p-5 md:p-6 space-y-4">
        <div className="flex items-center gap-2">
          <Globe size={16} className="text-[#cc543a]" />
          <h3 className="text-sm font-black uppercase tracking-widest">
            Global City Operations
          </h3>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-4 gap-3">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search city"
            className="border-2 border-black px-3 py-2 text-[10px] font-black uppercase tracking-widest outline-none"
          />
          <input
            value={countryCode}
            onChange={(e) => setCountryCode(e.target.value.toUpperCase())}
            placeholder="Country code (US, IN)"
            maxLength={2}
            className="border-2 border-black px-3 py-2 text-[10px] font-black uppercase tracking-widest outline-none"
          />
          <input
            type="number"
            min={1}
            max={500}
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value) || 100)}
            className="border-2 border-black px-3 py-2 text-[10px] font-black uppercase tracking-widest outline-none"
          />
          <button
            onClick={() => void loadCities()}
            className="bg-black text-white px-4 py-2 text-[10px] font-black uppercase flex items-center justify-center gap-2 hover:bg-[#cc543a] transition-colors"
          >
            {loading ? <Loader2 size={14} className="animate-spin" /> : <Search size={14} />}
            Load
          </button>
        </div>

        <div className="flex flex-wrap gap-3">
          <button
            onClick={handleDiscover}
            disabled={syncing}
            className="border-2 border-black px-4 py-2 text-[10px] font-black uppercase flex items-center gap-2 hover:bg-slate-100 disabled:opacity-50"
          >
            {syncing ? <Loader2 size={14} className="animate-spin" /> : <RefreshCw size={14} />}
            Discover via Nominatim
          </button>

          <button
            onClick={handleBootstrapCapitals}
            disabled={syncing}
            className="border-2 border-black px-4 py-2 text-[10px] font-black uppercase flex items-center gap-2 hover:bg-slate-100 disabled:opacity-50"
          >
            {syncing ? <Loader2 size={14} className="animate-spin" /> : <Rocket size={14} />}
            Bootstrap Capitals
          </button>
        </div>

        {lastSync && (
          <div className="border-2 border-black p-3 bg-slate-50 text-[10px] font-black uppercase tracking-widest">
            Processed {lastSync.processed} / Upserted {lastSync.upserted} / Failed {lastSync.failed}
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
        {loading ? (
          <div className="col-span-full flex justify-center py-10">
            <Loader2 size={28} className="animate-spin text-[#cc543a]" />
          </div>
        ) : cities.length === 0 ? (
          <div className="col-span-full text-center py-14 border-4 border-dashed border-black/10 font-black uppercase text-slate-400">
            No cities found for current filter
          </div>
        ) : (
          cities.map((city) => (
            <div key={city.id} className="border-2 border-black bg-white p-4 space-y-2">
              <div className="flex items-center justify-between gap-2">
                <p className="text-sm font-black uppercase tracking-tight break-words">
                  {city.name}
                </p>
                <span className="text-[9px] font-black uppercase px-2 py-1 border border-black">
                  {city.country_code}
                </span>
              </div>

              <p className="text-[9px] font-black uppercase text-slate-500">
                {city.center_lat?.toFixed(4) ?? "-"}, {city.center_lng?.toFixed(4) ?? "-"} / zoom {city.default_zoom ?? "-"}
              </p>

              <p className="text-xs text-slate-700 line-clamp-4 min-h-14">
                {city.description || "No encyclopedia summary yet"}
              </p>

              <p
                className={`text-[9px] font-black uppercase ${city.is_active ? "text-green-700" : "text-slate-400"}`}
              >
                {city.is_active ? "Active" : "Inactive"}
              </p>
            </div>
          ))
        )}
      </div>
    </div>
  );
};

export default AdminCitiesPanel;
