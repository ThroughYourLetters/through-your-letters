import React, { useMemo, useState } from "react";
import { RevisitLink } from "../types";

const BeforeAfterSlider: React.FC<{ revisit: RevisitLink }> = ({ revisit }) => {
  const [split, setSplit] = useState(50);

  const labels = useMemo(
    () => ({
      before: new Date(revisit.original.created_at).toLocaleDateString(),
      after: new Date(revisit.revisit.created_at).toLocaleDateString(),
    }),
    [revisit.original.created_at, revisit.revisit.created_at],
  );

  return (
    <div className="space-y-3 border-2 border-black p-4 bg-white">
      <div className="relative w-full aspect-square overflow-hidden border-2 border-black">
        <img
          src={revisit.original.image_url}
          alt="Original lettering"
          className="absolute inset-0 w-full h-full object-cover"
        />
        <div
          className="absolute inset-y-0 left-0 overflow-hidden"
          style={{ width: `${split}%` }}
        >
          <img
            src={revisit.revisit.image_url}
            alt="Revisit lettering"
            className="w-full h-full object-cover"
          />
        </div>
        <div
          className="absolute top-0 bottom-0 w-0.5 bg-white border-x border-black"
          style={{ left: `${split}%` }}
        />
      </div>
      <input
        type="range"
        min={0}
        max={100}
        value={split}
        onChange={(e) => setSplit(Number(e.target.value))}
        className="w-full"
      />
      <div className="flex justify-between text-[10px] font-black uppercase">
        <span>Before ({labels.before})</span>
        <span>After ({labels.after})</span>
      </div>
      {revisit.notes && (
        <p className="text-xs text-slate-700 border-l-2 border-black/20 pl-2">
          {revisit.notes}
        </p>
      )}
    </div>
  );
};

export default BeforeAfterSlider;
