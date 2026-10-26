import React from "react";
import { useSearchParams } from "react-router-dom";
import { SCRIPT_OPTIONS, STYLE_OPTIONS, SORT_OPTIONS } from "../types";
import { SlidersHorizontal } from "lucide-react";

const FilterBar: React.FC = () => {
  const [searchParams, setSearchParams] = useSearchParams();

  const script = searchParams.get("script") || "";
  const style = searchParams.get("style") || "";
  const sortBy = searchParams.get("sort") || "";

  const updateParam = (key: string, value: string) => {
    const next = new URLSearchParams(searchParams);
    if (value) {
      next.set(key, value);
    } else {
      next.delete(key);
    }
    setSearchParams(next, { replace: true });
  };

  const hasFilters = script || style || sortBy;

  return (
    <div className="flex items-center gap-2 overflow-x-auto pb-1 -mb-1">
      <SlidersHorizontal size={16} className="text-muted mr-1" />

      <select
        value={script}
        onChange={(e) => updateParam("script", e.target.value)}
        className={selectClass}
      >
        <option value="">All Scripts</option>
        {SCRIPT_OPTIONS.map((s) => (
          <option key={s} value={s}>{s}</option>
        ))}
      </select>

      <select
        value={style}
        onChange={(e) => updateParam("style", e.target.value)}
        className={selectClass}
      >
        <option value="">All Styles</option>
        {STYLE_OPTIONS.map((s) => (
          <option key={s} value={s}>{s}</option>
        ))}
      </select>

      <select
        value={sortBy}
        onChange={(e) => updateParam("sort", e.target.value)}
        className={selectClass}
      >
        <option value="">Sort: Default</option>
        {SORT_OPTIONS.map((s) => (
          <option key={s.value} value={s.value}>{s.label}</option>
        ))}
      </select>

      {hasFilters && (
        <button
          onClick={() => setSearchParams({}, { replace: true })}
          className="ml-auto shrink-0 whitespace-nowrap text-sm font-medium text-terracotta hover:text-terracotta-dark transition-colors"
        >
          Clear All
        </button>
      )}
    </div>
  );
};

const selectClass =
  "shrink-0 rounded-full border border-line bg-surface px-4 py-1.5 text-sm text-ink outline-none cursor-pointer hover:border-ink transition-colors";

export default FilterBar;
