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
    <div className="flex flex-wrap items-center gap-3 border-2 border-black bg-white p-4">
      <SlidersHorizontal size={16} className="text-slate-400" />

      <select
        value={script}
        onChange={(e) => updateParam("script", e.target.value)}
        className="border-2 border-black px-3 py-2 text-[10px] font-black uppercase tracking-widest bg-white outline-none cursor-pointer"
      >
        <option value="">All Scripts</option>
        {SCRIPT_OPTIONS.map((s) => (
          <option key={s} value={s}>{s}</option>
        ))}
      </select>

      <select
        value={style}
        onChange={(e) => updateParam("style", e.target.value)}
        className="border-2 border-black px-3 py-2 text-[10px] font-black uppercase tracking-widest bg-white outline-none cursor-pointer"
      >
        <option value="">All Styles</option>
        {STYLE_OPTIONS.map((s) => (
          <option key={s} value={s}>{s}</option>
        ))}
      </select>

      <select
        value={sortBy}
        onChange={(e) => updateParam("sort", e.target.value)}
        className="border-2 border-black px-3 py-2 text-[10px] font-black uppercase tracking-widest bg-white outline-none cursor-pointer"
      >
        <option value="">Sort: Default</option>
        {SORT_OPTIONS.map((s) => (
          <option key={s.value} value={s.value}>{s.label}</option>
        ))}
      </select>

      {hasFilters && (
        <button
          onClick={() => setSearchParams({}, { replace: true })}
          className="ml-auto text-[10px] font-black uppercase text-[#cc543a] hover:text-black transition-colors"
        >
          Clear All
        </button>
      )}
    </div>
  );
};

export default FilterBar;
