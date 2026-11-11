import { useId, useState } from "react";
import { X, ZoomIn, ZoomOut, Download } from "lucide-react";
import { API_BASE_URL } from "../constants";
import ModalDialog from "./ModalDialog";

export default function ImageLightbox({
  imageUrl,
  title,
  letteringId,
  onClose,
}: {
  imageUrl: string;
  title: string;
  letteringId?: string | number;
  onClose: () => void;
}) {
  const titleId = useId();
  const [scale, setScale] = useState(1);
  return (
    <ModalDialog
      titleId={titleId}
      onClose={onClose}
      className="m-0 p-0 border-0 w-screen h-[100dvh] max-w-none max-h-none bg-black/95 text-white"
    >
      <div
        className="h-full flex flex-col"
        onKeyDown={(event) => {
          if (["+", "=", "-"].includes(event.key)) {
            event.preventDefault();
            setScale((value) =>
              Math.max(
                0.5,
                Math.min(4, value + (event.key === "-" ? -0.25 : 0.25)),
              ),
            );
          }
        }}
      >
        <div className="shrink-0 flex justify-end gap-2 p-4">
          <button
            type="button"
            aria-label="Zoom in"
            disabled={scale >= 4}
            onClick={() => setScale((value) => Math.min(value + 0.25, 4))}
            className="p-3 bg-white/10 hover:bg-white/20 disabled:opacity-50"
          >
            <ZoomIn size={20} aria-hidden="true" />
          </button>
          <button
            type="button"
            aria-label="Zoom out"
            disabled={scale <= 0.5}
            onClick={() => setScale((value) => Math.max(value - 0.25, 0.5))}
            className="p-3 bg-white/10 hover:bg-white/20 disabled:opacity-50"
          >
            <ZoomOut size={20} aria-hidden="true" />
          </button>
          {letteringId && (
            <a
              href={`${API_BASE_URL}/api/v1/letterings/${letteringId}/download`}
              aria-label="Download high-resolution image"
              className="p-3 bg-white/10 hover:bg-white/20"
            >
              <Download size={20} aria-hidden="true" />
            </a>
          )}
          <button
            type="button"
            aria-label="Close image"
            onClick={onClose}
            className="p-3 bg-white/10 hover:bg-white/20"
          >
            <X size={20} aria-hidden="true" />
          </button>
        </div>
        <div
          className="min-h-0 flex-1 overflow-auto flex items-center justify-center"
          onClick={(event) => {
            if (event.target === event.currentTarget) onClose();
          }}
        >
          <img
            src={imageUrl}
            alt={title}
            draggable={false}
            className="max-w-[90vw] max-h-full object-contain transition-transform duration-200"
            style={{ transform: `scale(${scale})` }}
          />
        </div>
        <h2
          id={titleId}
          className="shrink-0 px-4 py-4 text-center text-sm text-white/80 break-words"
        >
          {title} — Esc to close
        </h2>
      </div>
    </ModalDialog>
  );
}
