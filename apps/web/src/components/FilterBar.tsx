import React, { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { SCRIPT_OPTIONS, STYLE_OPTIONS, SORT_OPTIONS } from "../types";
import { X } from "lucide-react";

const useFilterParams = () => {
  const [searchParams, setSearchParams] = useSearchParams();

  const updateParam = (key: string, value: string) => {
    const next = new URLSearchParams(searchParams);
    if (value) {
      next.set(key, value);
    } else {
      next.delete(key);
    }
    setSearchParams(next, { replace: true });
  };

  return {
    script: searchParams.get("script") || "",
    style: searchParams.get("style") || "",
    sortBy: searchParams.get("sort") || "",
    updateParam,
    clearAll: () => setSearchParams({}, { replace: true }),
  };
};

// Style and sort dropdowns, plus a clear button once any filter is set
export const FilterControls: React.FC<{ className?: string }> = ({
  className = "",
}) => {
  const { script, style, sortBy, updateParam, clearAll } = useFilterParams();
  const hasFilters = script || style || sortBy;

  return (
    <div className={`flex shrink-0 items-center gap-2 ${className}`}>
      <select
        value={style}
        onChange={(e) => updateParam("style", e.target.value)}
        className={selectClass}
        aria-label="Style"
      >
        <option value="">All styles</option>
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

      <button
        onClick={clearAll}
        className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-full text-muted transition-all duration-200 hover:bg-ink/5 hover:text-ink ${
          hasFilters ? "scale-100 opacity-100" : "pointer-events-none scale-75 opacity-0"
        }`}
        aria-label="Clear all filters"
        aria-hidden={!hasFilters}
        tabIndex={hasFilters ? 0 : -1}
        title="Clear all filters"
      >
        <X size={16} />
      </button>
    </div>
  );
};

// Script chips. They wrap on wider screens and scroll sideways on phones,
// fading only the edge that still has more chips behind it.
export const ScriptPills: React.FC = () => {
  const { script, updateParam } = useFilterParams();
  const rowRef = useRef<HTMLDivElement>(null);
  const [edges, setEdges] = useState({ start: false, end: false });

  useEffect(() => {
    const row = rowRef.current;
    if (!row) return;
    const update = () =>
      setEdges({
        start: row.scrollLeft > 4,
        end: row.scrollLeft + row.clientWidth < row.scrollWidth - 4,
      });
    update();
    row.addEventListener("scroll", update, { passive: true });
    const observer = new ResizeObserver(update);
    observer.observe(row);
    return () => {
      row.removeEventListener("scroll", update);
      observer.disconnect();
    };
  }, []);

  // Keep the selected chip in view when it sits off screen
  useEffect(() => {
    const row = rowRef.current;
    const active = row?.querySelector<HTMLElement>("[aria-pressed='true']");
    if (!row || !active) return;
    const left = active.offsetLeft - row.offsetLeft;
    const right = left + active.offsetWidth;
    if (left < row.scrollLeft || right > row.scrollLeft + row.clientWidth) {
      row.scrollTo({ left: left - 24, behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
    }
  }, [script]);

  const mask =
    edges.start && edges.end
      ? "linear-gradient(to right, transparent, black 2rem, black calc(100% - 2rem), transparent)"
      : edges.end
        ? "linear-gradient(to right, black calc(100% - 2rem), transparent)"
        : edges.start
          ? "linear-gradient(to right, transparent, black 2rem)"
          : undefined;

  const chips = [{ value: "", label: "All scripts" }, ...SCRIPT_OPTIONS.map((s) => ({ value: s, label: s }))];

  return (
    <div
      ref={rowRef}
      style={{ maskImage: mask, WebkitMaskImage: mask }}
      className="no-scrollbar -my-1 flex items-center gap-1.5 overflow-x-auto py-1 sm:flex-wrap sm:overflow-visible"
      role="group"
      aria-label="Filter by script"
    >
      {chips.map(({ value, label }) => {
        const active = script === value;
        return (
          <button
            key={label}
            onClick={() => updateParam("script", active && value ? "" : value)}
            className={`chip-toggle ${active ? "chip-toggle-active" : ""}`}
            aria-pressed={active}
          >
            {label}
          </button>
        );
      })}
    </div>
  );
};

const selectClass =
  "h-9 max-w-[9.5rem] shrink-0 cursor-pointer rounded-full border border-line bg-surface px-3 text-sm text-ink outline-none transition-colors duration-200 hover:border-ink focus-visible:border-ink";

const FilterBar: React.FC = () => (
  <div className="flex flex-col gap-3">
    <ScriptPills />
    <FilterControls />
  </div>
);

export default FilterBar;
