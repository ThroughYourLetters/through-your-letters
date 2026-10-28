import React, { useState, useEffect } from "react";
import { Search, X, Loader2 } from "lucide-react";
import { Lettering } from "../types";
import { api } from "../lib/api";
import { useLocaleStore } from "../store/useLocaleStore";

interface SearchBarProps {
  onResults: (results: Lettering[]) => void;
  onClear: () => void;
}

const SearchBar: React.FC<SearchBarProps> = ({ onResults, onClear }) => {
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [resultCount, setResultCount] = useState<number | null>(null);
  const [error, setError] = useState(false);
  const { locale, t } = useLocaleStore();

  useEffect(() => {
    const controller = new AbortController();
    setError(false);
    if (!query.trim()) {
      setLoading(false);
      setResultCount(null);
      onClear();
      return () => controller.abort();
    }

    const timer = setTimeout(async () => {
      setLoading(true);
      try {
        const results = await api.search(query.trim(), locale, controller.signal);
        if (controller.signal.aborted) return;
        setResultCount(results.length);
        onResults(results);
      } catch {
        if (!controller.signal.aborted) setError(true);
      } finally {
        if (!controller.signal.aborted) setLoading(false);
      }
    }, 400);

    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [query, locale, onClear, onResults]);

  const handleClear = () => {
    setQuery("");
    setResultCount(null);
    setError(false);
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

      {error && <p role="alert" className="mt-2 pl-5 text-sm text-red-700">Search unavailable. Please try again.</p>}
      {resultCount !== null && (
        <p className="md:hidden mt-2 pl-5 text-xs text-muted">
          Showing {resultCount} archive entries
        </p>
      )}
    </div>
  );
};

export default SearchBar;
