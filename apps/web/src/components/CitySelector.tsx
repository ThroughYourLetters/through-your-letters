import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useQuery } from "@tanstack/react-query";
import { Check, ChevronDown, Loader2, MapPin, Search } from "lucide-react";
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
  const [searchError, setSearchError] = useState<string | null>(null);
  const requestRef = useRef<AbortController | null>(null);
  const savedCities = useQuery({
    queryKey: ["cities"],
    queryFn: ({ signal }) => api.getCities({ limit: 300, signal }),
  });

  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  const { selectedCityId, selectedCityName, setCity, clearCity } =
    useCityStore();

  // Close the popover on outside click or Escape
  useEffect(() => {
    if (!open) return;
    const onPointer = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        triggerRef.current?.focus();
      }
    };
    document.addEventListener("mousedown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const search = useCallback(
    async (query: string, discover: boolean, controller: AbortController) => {
      if (controller.signal.aborted) return;
      setSearching(true);
      setSearchError(null);
      try {
        const data = await api.searchCities(query, 30, {
          discover,
          signal: controller.signal,
        });
        if (controller.signal.aborted) return;
        setCities((previous) => mergeUniqueCities(previous, data));
        if (discover && data.length === 0)
          setSearchError("No cities found. Try another name.");
      } catch {
        if (!controller.signal.aborted)
          setSearchError("Could not search cities. Try again.");
      } finally {
        if (!controller.signal.aborted) setSearching(false);
      }
    },
    [],
  );

  useEffect(() => {
    requestRef.current?.abort();
    const controller = new AbortController();
    requestRef.current = controller;
    setSearching(false);
    setSearchError(null);
    const query = searchQuery.trim();
    // Typing must never call public geocoders.
    const timer =
      open && query.length >= 2
        ? window.setTimeout(() => void search(query, false, controller), 450)
        : undefined;
    return () => {
      window.clearTimeout(timer);
      controller.abort();
      requestRef.current?.abort();
    };
  }, [searchQuery, open, search]);

  const searchWorldwide = () => {
    requestRef.current?.abort();
    const controller = new AbortController();
    requestRef.current = controller;
    void search(searchQuery.trim(), true, controller);
  };

  const activeCities = useMemo(
    () =>
      mergeUniqueCities(savedCities.data ?? [], cities).filter(
        (c) => c.is_active !== false,
      ),
    [cities, savedCities.data],
  );

  const visibleCities = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return activeCities;
    return activeCities.filter(
      (c) =>
        c.name.toLowerCase().includes(q) || c.country_code.toLowerCase() === q,
    );
  }, [activeCities, searchQuery]);

  const selectCity = (city: CityOption | null) => {
    if (!city) {
      clearCity();
    } else {
      setCity(
        city.id,
        city.name,
        city.center_lat ?? 0,
        city.center_lng ?? 0,
        city.default_zoom ?? 11,
      );
    }
    setOpen(false);
    setSearchQuery("");
    triggerRef.current?.focus();
  };

  return (
    <div ref={rootRef} className="relative">
      <button
        ref={triggerRef}
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="dialog"
        aria-expanded={open}
        className="inline-flex max-w-[11rem] items-center gap-1.5 rounded-full border border-line bg-surface px-3 py-1.5 text-sm font-medium text-ink transition-colors hover:border-ink"
      >
        <MapPin size={14} className="shrink-0 text-terracotta" />
        <span className="truncate">{selectedCityName}</span>
        <ChevronDown
          size={14}
          className={`shrink-0 text-muted transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>

      {open && (
        <div
          role="dialog"
          aria-label="Choose a city"
          className="fixed inset-x-4 top-16 z-50 mt-2 max-w-[calc(100vw-2rem)] sm:absolute sm:inset-x-auto sm:right-0 sm:top-full sm:w-72 overflow-hidden rounded-2xl border border-line bg-surface shadow-lift animate-in"
        >
          <div className="relative border-b border-line p-2">
            <Search
              size={14}
              className="absolute left-5 top-1/2 -translate-y-1/2 text-muted"
            />
            <input
              autoFocus
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search city worldwide"
              aria-label="Search city worldwide"
              className="w-full rounded-xl border-0 bg-paper py-2 pl-9 pr-8 text-sm outline-none placeholder:text-muted/70"
            />
            {(searching || savedCities.isLoading) && (
              <Loader2
                size={14}
                className="absolute right-5 top-1/2 -translate-y-1/2 animate-spin text-terracotta"
              />
            )}
          </div>
          {searchQuery.trim().length >= 2 && (
            <button
              type="button"
              onClick={searchWorldwide}
              disabled={searching}
              className="w-full border-b border-line px-3 py-2 text-left text-sm text-terracotta disabled:opacity-50"
            >
              Search worldwide
            </button>
          )}
          {savedCities.isLoading && (
            <p role="status" className="px-3 py-2 text-sm text-muted">
              Loading cities...
            </p>
          )}
          {searchError && (
            <p role="status" className="px-3 py-2 text-sm text-muted">
              {searchError}
            </p>
          )}
          {savedCities.isError && (
            <div role="alert" className="px-3 py-2 text-sm text-muted">
              Could not load saved cities.{" "}
              <button
                type="button"
                className="underline"
                onClick={() => void savedCities.refetch()}
              >
                Retry cities
              </button>
            </div>
          )}
          <ul aria-label="Cities" className="max-h-72 overflow-y-auto p-1.5">
            <CityRow
              label="All Cities"
              active={!selectedCityId}
              onClick={() => selectCity(null)}
            />
            {visibleCities.map((c) => (
              <CityRow
                key={c.id}
                label={c.name}
                hint={c.country_code}
                active={selectedCityId === c.id}
                onClick={() => selectCity(c)}
              />
            ))}
            {visibleCities.length === 0 &&
              !searching &&
              !savedCities.isLoading &&
              !searchError && (
                <li className="px-3 py-4 text-center text-sm text-muted">
                  No matching cities
                </li>
              )}
          </ul>
        </div>
      )}
    </div>
  );
};

const CityRow: React.FC<{
  label: string;
  hint?: string;
  active: boolean;
  onClick: () => void;
}> = ({ label, hint, active, onClick }) => (
  <li>
    <button
      onClick={onClick}
      aria-pressed={active}
      className={`flex w-full items-center justify-between gap-3 rounded-xl px-3 py-2 text-left text-sm transition-colors hover:bg-paper ${active ? "font-semibold text-ink" : "text-ink/80"}`}
    >
      <span className="truncate">
        {label}
        {hint && <span className="ml-1.5 text-xs text-muted">{hint}</span>}
      </span>
      {active && <Check size={14} className="shrink-0 text-terracotta" />}
    </button>
  </li>
);

export default CitySelector;
