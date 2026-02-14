import React, { useEffect, useMemo, useState } from "react";
import { Globe, Loader2, MapPin } from "lucide-react";
import { api } from "../lib/api";
import { useCityStore } from "../store/useCityStore";

interface CityOption {
  id: string;
  name: string;
  country_code: string;
  center_lat: number | null;
  center_lng: number | null;
  default_zoom: number | null;
  is_active: boolean | null;
}

const mergeUniqueCities = (base: CityOption[], incoming: CityOption[]) => {
  const byId = new Map<string, CityOption>();
  for (const city of base) byId.set(city.id, city);
  for (const city of incoming) byId.set(city.id, city);
  return Array.from(byId.values()).sort((a, b) => {
    const activeA = a.is_active ? 1 : 0;
    const activeB = b.is_active ? 1 : 0;
    if (activeA !== activeB) return activeB - activeA;
    return a.name.localeCompare(b.name);
  });
};

const CitySelector: React.FC = () => {
  const [cities, setCities] = useState<CityOption[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [searching, setSearching] = useState(false);

  const { selectedCityId, setCity, clearCity } = useCityStore();

  useEffect(() => {
    let mounted = true;
    api
      .getCities({ limit: 300 })
      .then((data) => {
        if (mounted) setCities(data);
      })
      .catch(() => {});

    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => {
    const q = searchQuery.trim();
    if (q.length < 2) return;

    const timer = window.setTimeout(() => {
      setSearching(true);
      api
        .searchCities(q, 30)
        .then((data) => {
          setCities((prev) => mergeUniqueCities(prev, data));
        })
        .catch(() => {})
        .finally(() => setSearching(false));
    }, 450);

    return () => window.clearTimeout(timer);
  }, [searchQuery]);

  const activeCities = useMemo(
    () => cities.filter((c) => c.is_active !== false),
    [cities],
  );

  const handleChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const val = e.target.value;
    if (val === "all") {
      clearCity();
      return;
    }

    const city = cities.find((c) => c.id === val);
    if (!city) return;

    setCity(
      city.id,
      city.name,
      city.center_lat ?? 0,
      city.center_lng ?? 0,
      city.default_zoom ?? 11,
    );
  };

  return (
    <div className="flex items-center gap-2">
      <MapPin size={14} className="text-[#cc543a]" />
      <div className="flex flex-col gap-1 min-w-[190px]">
        <div className="relative">
          <Globe size={12} className="absolute left-2 top-1/2 -translate-y-1/2 text-slate-400" />
          <input
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search city worldwide"
            className="w-full pl-6 pr-6 py-1 bg-transparent border-b border-black text-[9px] font-black uppercase tracking-widest outline-none"
          />
          {searching && (
            <Loader2
              size={12}
              className="absolute right-1 top-1/2 -translate-y-1/2 animate-spin text-[#cc543a]"
            />
          )}
        </div>
        <select
          value={selectedCityId || "all"}
          onChange={handleChange}
          className="bg-transparent border-b-2 border-black text-[10px] font-black uppercase tracking-widest outline-none cursor-pointer py-1 pr-6"
        >
          <option value="all">All Cities</option>
          {activeCities.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name} ({c.country_code})
            </option>
          ))}
        </select>
      </div>
    </div>
  );
};

export default CitySelector;
