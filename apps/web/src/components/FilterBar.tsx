import React from "react";
import { useSearchParams } from "react-router-dom";
import { SCRIPT_OPTIONS, STYLE_OPTIONS, SORT_OPTIONS } from "../types";
import { SlidersHorizontal, X } from "lucide-react";

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
    <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
      <div
        className="no-scrollbar min-w-0 -my-1 flex flex-1 [mask-image:linear-gradient(to_right,black_92%,transparent)] items-center gap-1.5 overflow-x-auto py-1"
        role="group"
        aria-label="Filter by script"
      >
        <button
          onClick={() => updateParam("script", "")}
          className={`chip-toggle ${!script ? "chip-toggle-active" : ""}`}
          aria-pressed={!script}
        >
          All scripts
        </button>
        {SCRIPT_OPTIONS.map((s) => (
          <button
            key={s}
            onClick={() => updateParam("script", script === s ? "" : s)}
            className={`chip-toggle ${script === s ? "chip-toggle-active" : ""}`}
            aria-pressed={script === s}
          >
            {s}
          </button>
        ))}
      </div>

      <div className="flex shrink-0 items-center gap-2 sm:border-l sm:border-line sm:pl-2">
        <SlidersHorizontal size={16} className="hidden sm:block text-muted" />
        <select
          value={style}
          onChange={(e) => updateParam("style", e.target.value)}
          className={selectClass}
          aria-label="Style"
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
          aria-label="Sort"
        >
          <option value="">Sort: Default</option>
          {SORT_OPTIONS.map((s) => (
            <option key={s.value} value={s.value}>{s.label}</option>
          ))}
        </select>

        {hasFilters && (
          <button
            onClick={() => setSearchParams({}, { replace: true })}
            className="flex h-8 w-8 items-center justify-center rounded-full text-muted hover:bg-ink/5 hover:text-ink"
            aria-label="Clear all filters"
            title="Clear all filters"
          >
            <X size={16} />
          </button>
        )}
      </div>
    </div>
  );
};

const selectClass =
  "max-w-[9.5rem] shrink-0 rounded-full border border-line bg-surface px-3 py-1.5 text-sm text-ink outline-none cursor-pointer hover:border-ink transition-colors";

export default FilterBar;
