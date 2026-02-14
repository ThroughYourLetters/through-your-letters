import React, { useState, useEffect, useRef } from "react";
import { Search, X, Loader2 } from "lucide-react";
import { api } from "../lib/api";
import { useLocaleStore } from "../store/useLocaleStore";

interface SearchBarProps {
  onResults: (results: any[]) => void;
  onClear: () => void;
}

const SearchBar: React.FC<SearchBarProps> = ({ onResults, onClear }) => {
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [resultCount, setResultCount] = useState<number | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const { locale, t } = useLocaleStore();

  useEffect(() => {
    // Clear existing timer
    if (debounceRef.current) clearTimeout(debounceRef.current);

    if (!query.trim()) {
      setResultCount(null);
      onClear();
      return;
    }

    // Debounce API call by 400ms
    debounceRef.current = setTimeout(async () => {
      setLoading(true);
      try {
        const results = await api.search(query.trim(), locale);
        setResultCount(results.length);
        onResults(results);
      } catch (error) {
        console.error("Search failed:", error);
        setResultCount(0);
        onResults([]);
      } finally {
        setLoading(false);
      }
    }, 400);

    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [query, locale, onClear, onResults]);

  const handleClear = () => {
    setQuery("");
    setResultCount(null);
    onClear();
  };

  return (
    <div className="relative">
      <div className="flex items-center border-4 border-black bg-white brutalist-shadow-sm transition-all focus-within:brutalist-shadow">
        <div className="pl-4 text-slate-400">
          {loading ? (
            <Loader2 size={20} className="animate-spin text-[#cc543a]" />
          ) : (
            <Search size={20} />
          )}
        </div>
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("search_placeholder")}
          className="flex-1 p-4 font-black text-sm md:text-base outline-none bg-transparent placeholder:text-slate-300"
        />
        {query && (
          <button
            onClick={handleClear}
            className="pr-4 text-slate-400 hover:text-black transition-colors"
          >
            <X size={20} />
          </button>
        )}
        {resultCount !== null && (
          <div className="pr-4 hidden md:block">
            <span className="bg-black text-white text-[9px] font-black uppercase px-2 py-1 tracking-widest">
              {resultCount} Result{resultCount !== 1 ? "s" : ""}
            </span>
          </div>
        )}
      </div>

      {/* Mobile result count badge */}
      {resultCount !== null && (
        <div className="md:hidden mt-2">
          <span className="text-[8px] font-black uppercase text-slate-400 tracking-widest">
            Showing {resultCount} archive entries
          </span>
        </div>
      )}
    </div>
  );
};

export default SearchBar;
