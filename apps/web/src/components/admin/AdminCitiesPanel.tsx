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
        `${result.upserted} cities added or updated; ${result.failed} failed`,
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
        "Add capital cities? Existing city records will also be updated.",
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
        `${result.upserted} capital cities added or updated; ${result.failed} failed`,
        result.failed > 0 ? "warning" : "success",
      );
      await loadCities();
    } catch (e) {
      addToast((e as Error).message || "Could not add capital cities", "error");
    } finally {
      setSyncing(false);
    }
  };

  return (
    <div className="space-y-8">
      <div className="card p-5 md:p-6 space-y-4">
        <div className="flex items-center gap-2">
          <Globe size={16} className="text-terracotta" />
          <h3 className="text-sm font-semibold uppercase tracking-widest">
            Global City Operations
          </h3>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-4 gap-3">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search city"
            className="border border-line px-3 py-2 text-xs font-semibold uppercase tracking-widest outline-none"
          />
          <input
            value={countryCode}
            onChange={(e) => setCountryCode(e.target.value.toUpperCase())}
            placeholder="Country code (US, IN)"
            maxLength={2}
            className="border border-line px-3 py-2 text-xs font-semibold uppercase tracking-widest outline-none"
          />
          <input
            type="number"
            min={1}
            max={500}
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value) || 100)}
            className="border border-line px-3 py-2 text-xs font-semibold uppercase tracking-widest outline-none"
          />
          <button
            onClick={() => void loadCities()}
            className="btn-dark justify-center"
          >
            {loading ? <Loader2 size={14} className="animate-spin" /> : <Search size={14} />}
            Load
          </button>
        </div>

        <div className="flex flex-wrap gap-3">
          <button
            onClick={handleDiscover}
            disabled={syncing}
            className="border border-line px-4 py-2 text-xs font-semibold uppercase flex items-center gap-2 hover:bg-stone-100 disabled:opacity-50"
          >
            {syncing ? <Loader2 size={14} className="animate-spin" /> : <RefreshCw size={14} />}
            Find cities
          </button>

          <button
            onClick={handleBootstrapCapitals}
            disabled={syncing}
            className="border border-line px-4 py-2 text-xs font-semibold uppercase flex items-center gap-2 hover:bg-stone-100 disabled:opacity-50"
          >
            {syncing ? <Loader2 size={14} className="animate-spin" /> : <Rocket size={14} />}
            Add capital cities
          </button>
        </div>

        {lastSync && (
          <div className="border border-line p-3 bg-stone-50 text-xs font-semibold uppercase tracking-widest">
            Checked {lastSync.processed} / Added or updated {lastSync.upserted} / Failed {lastSync.failed}
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
        {loading ? (
          <div className="col-span-full flex justify-center py-10">
            <Loader2 size={28} className="animate-spin text-terracotta" />
          </div>
        ) : cities.length === 0 ? (
          <div className="col-span-full text-center py-14 border border-dashed border-line font-semibold uppercase text-stone-400">
            No cities found for current filter
          </div>
        ) : (
          cities.map((city) => (
            <div key={city.id} className="card p-4 space-y-2">
              <div className="flex items-center justify-between gap-2">
                <p className="text-sm font-semibold uppercase tracking-tight break-words">
                  {city.name}
                </p>
                <span className="text-xs font-semibold uppercase px-2 py-1 border border-line">
                  {city.country_code}
                </span>
              </div>

              <p className="text-xs font-semibold uppercase text-stone-500">
                {city.center_lat?.toFixed(4) ?? "-"}, {city.center_lng?.toFixed(4) ?? "-"} / zoom {city.default_zoom ?? "-"}
              </p>

              <p className="text-xs text-stone-700 line-clamp-4 min-h-14">
                {city.description || "No encyclopedia summary yet"}
              </p>

              <p
                className={`text-xs font-semibold uppercase ${city.is_active ? "text-green-700" : "text-stone-400"}`}
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
