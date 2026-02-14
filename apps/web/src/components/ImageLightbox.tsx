import React, { useEffect, useState, useCallback } from "react";
import { X, ZoomIn, ZoomOut, Download } from "lucide-react";
import { API_BASE_URL } from "../constants";

const ImageLightbox: React.FC<{
  imageUrl: string;
  title: string;
  letteringId?: string | number;
  onClose: () => void;
}> = ({ imageUrl, title, letteringId, onClose }) => {
  const [scale, setScale] = useState(1);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (e.key === "+" || e.key === "=") setScale((s) => Math.min(s + 0.25, 4));
      if (e.key === "-") setScale((s) => Math.max(s - 0.25, 0.5));
    },
    [onClose],
  );

  useEffect(() => {
    document.addEventListener("keydown", handleKeyDown);
    document.body.style.overflow = "hidden";
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.body.style.overflow = "";
    };
  }, [handleKeyDown]);

  return (
    <div
      className="fixed inset-0 z-[100] bg-black/95 flex items-center justify-center"
      onClick={onClose}
    >
      <div className="absolute top-4 right-4 flex gap-2 z-10">
        <button
          onClick={(e) => {
            e.stopPropagation();
            setScale((s) => Math.min(s + 0.25, 4));
          }}
          className="p-2 bg-white/10 text-white hover:bg-white/20 transition-colors"
        >
          <ZoomIn size={20} />
        </button>
        <button
          onClick={(e) => {
            e.stopPropagation();
            setScale((s) => Math.max(s - 0.25, 0.5));
          }}
          className="p-2 bg-white/10 text-white hover:bg-white/20 transition-colors"
        >
          <ZoomOut size={20} />
        </button>
        {letteringId && (
          <a
            href={`${API_BASE_URL}/api/v1/letterings/${letteringId}/download`}
            onClick={(e) => e.stopPropagation()}
            className="p-2 bg-white/10 text-white hover:bg-white/20 transition-colors"
            title="Download high-res"
          >
            <Download size={20} />
          </a>
        )}
        <button
          onClick={onClose}
          className="p-2 bg-white/10 text-white hover:bg-white/20 transition-colors"
        >
          <X size={20} />
        </button>
      </div>

      <img
        src={imageUrl}
        alt={title}
        className="max-w-[90vw] max-h-[90vh] object-contain transition-transform duration-200"
        style={{ transform: `scale(${scale})` }}
        onClick={(e) => e.stopPropagation()}
        draggable={false}
      />

      <p className="absolute bottom-6 left-1/2 -translate-x-1/2 text-white/60 text-[10px] font-black uppercase tracking-widest">
        {title} — Esc to close
      </p>
    </div>
  );
};

export default ImageLightbox;
