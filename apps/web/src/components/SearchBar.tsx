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
      <div className="flex items-center rounded-full border border-line bg-surface shadow-soft transition-all focus-within:border-terracotta focus-within:shadow-card">
        <div className="pl-5 text-muted">
          {loading ? (
            <Loader2 size={18} className="animate-spin text-terracotta" />
          ) : (
            <Search size={18} />
          )}
        </div>
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("search_placeholder")}
          aria-label={t("search_placeholder")}
          className="flex-1 bg-transparent px-3 py-3 text-sm md:text-base outline-none placeholder:text-muted/70 focus-visible:!shadow-none"
        />
        {resultCount !== null && (
          <span className="hidden md:inline-flex chip mr-2">
            {resultCount} result{resultCount !== 1 ? "s" : ""}
          </span>
        )}
        {query && (
          <button
            onClick={handleClear}
            className="mr-2 flex h-8 w-8 items-center justify-center rounded-full text-muted hover:bg-ink/5 hover:text-ink transition-colors"
            aria-label="Clear search"
          >
            <X size={16} />
          </button>
        )}
      </div>

      {resultCount !== null && (
        <p className="md:hidden mt-2 pl-5 text-xs text-muted">
          Showing {resultCount} archive entries
        </p>
      )}
    </div>
  );
};

export default SearchBar;
