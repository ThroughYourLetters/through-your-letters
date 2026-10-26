import React, { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Loader2, AlertCircle, Filter } from "lucide-react";
import LetteringCard from "./LetteringCard";
import { api } from "../lib/api";
import { Lettering } from "../types";

const Gallery: React.FC = () => {
  const [limit] = useState(50);
  const [offset, setOffset] = useState(0);

  const { data, isLoading, error } = useQuery({
    queryKey: ["letterings", limit, offset],
    queryFn: () => api.getGallery({ limit, offset }),
  });

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-20">
        <Loader2 size={48} className="animate-spin text-terracotta" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="bg-red-100 border border-red-600 p-8 text-center">
        <AlertCircle size={48} className="mx-auto mb-4 text-red-600" />
        <p className="text-lg font-semibold uppercase text-red-800">
          Failed to load gallery
        </p>
        <p className="text-sm text-red-700 mt-2">
          {error instanceof Error ? error.message : "Unknown error"}
        </p>
      </div>
    );
  }

  const letterings = data?.letterings || [];

  if (letterings.length === 0) {
    return (
      <div className="bg-stone-100 border border-line p-12 text-center">
        <p className="text-lg font-semibold uppercase text-stone-600">
          No letterings yet
        </p>
        <p className="text-sm text-stone-500 mt-2">
          Be the first to contribute!
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-8">
      <div className="flex flex-col md:flex-row justify-between items-start md:items-end border-b border-line pb-6">
        <div>
          <h2 className="font-display font-normal text-4xl md:text-6xl tracking-tight leading-none">
            The Gallery
          </h2>
          <p className="text-xs font-semibold uppercase tracking-widest text-stone-400 mt-2">
            {letterings.length} specimens archived
          </p>
        </div>
        <button className="mt-4 md:mt-0 flex items-center gap-2 px-4 py-2 bg-surface border border-line text-xs font-semibold uppercase tracking-widest hover:bg-stone-100">
          <Filter size={16} />
          Filter
        </button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        {letterings.map((lettering: Lettering) => (
          <LetteringCard key={lettering.id} lettering={lettering} />
        ))}
      </div>

      {data && data.total > limit && (
        <div className="flex justify-center gap-4 pt-8">
          <button
            onClick={() => setOffset(Math.max(0, offset - limit))}
            disabled={offset === 0}
            className="px-6 py-3 bg-ink text-white font-semibold uppercase text-sm disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Previous
          </button>
          <button
            onClick={() => setOffset(offset + limit)}
            disabled={offset + limit >= data.total}
            className="px-6 py-3 bg-ink text-white font-semibold uppercase text-sm disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Next
          </button>
        </div>
      )}
    </div>
  );
};

export default Gallery;
