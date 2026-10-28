import type L from "leaflet";
import { Popup } from "react-leaflet";
import { useCallback, useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { api } from "../lib/api";
import { mapLimits } from "../lib/runtimeConfig";
import type { DensityCell, MapFilters } from "../types/mapDensity";

export default function MapHotspot({
  cell,
  filters,
  position,
  onClose,
}: {
  cell: DensityCell;
  filters: MapFilters;
  position: [number, number];
  onClose: () => void;
}) {
  const [cursors, setCursors] = useState<Array<string | undefined>>([
    undefined,
  ]);
  const after = cursors.at(-1);
  const region = useRef<HTMLElement | null>(null);
  const attachRegion = useCallback((node: HTMLElement | null) => {
    region.current = node;
    node?.focus({ preventScroll: true });
  }, []);
  const popup = useRef<L.Popup>(null);
  useEffect(() => {
    region.current?.focus({ preventScroll: true });
  }, [after]);
  const photos = useQuery({
    queryKey: ["map", "photos", cell.bounds, filters, after],
    queryFn: ({ signal }) =>
      api.getMapPhotos(cell.bounds, filters, after, signal),
    gcTime: mapLimits.query_gc_seconds * 1000,
  });
  useEffect(() => {
    // Keep changing photo pages inside bounds.
    popup.current?.update();
    region.current?.focus({ preventScroll: true });
  }, [photos.data, photos.isLoading, photos.isError]);
  return (
    <Popup
      ref={popup}
      position={position}
      maxWidth={240}
      minWidth={180}
      autoPanPadding={[35, 35]}
      eventHandlers={{ remove: onClose }}
    >
      <section
        ref={attachRegion}
        onClick={(event) => event.stopPropagation()}
        onDoubleClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          event.stopPropagation();
          if (event.key === "Escape") popup.current?.remove();
        }}
        tabIndex={-1}
        aria-label="Hotspot uploads"
        className="space-y-2"
      >
        <h3 className="font-semibold">Uploads in this area</h3>
        <p className="text-xs">Approximate postal locations</p>
        {photos.isLoading && <p role="status">Loading photos...</p>}
        {photos.isError && (
          <p role="alert">
            Could not load photos.{" "}
            <button onClick={() => void photos.refetch()} className="underline">
              Retry photos
            </button>
          </p>
        )}
        {photos.data && (
          <>
            {photos.data.items.length === 0 && (
              <p>No uploads here match these filters.</p>
            )}
            <ul className="max-h-60 overflow-y-auto space-y-2">
              {photos.data.items.map((photo, index) => (
                <li key={photo.id}>
                  <Link
                    to={`/lettering/${encodeURIComponent(photo.id)}`}
                    className="flex items-center gap-2"
                    aria-label={`View upload ${index + 1}, ${photo.pin_code}`}
                  >
                    {photo.thumbnail && (
                      <img
                        src={photo.thumbnail}
                        alt=""
                        loading="lazy"
                        className="w-14 h-14 rounded object-cover"
                      />
                    )}
                    <span>
                      View upload {index + 1}
                      <small className="block">{photo.pin_code}</small>
                    </span>
                  </Link>
                </li>
              ))}
            </ul>
            <div className="flex gap-3">
              <button
                disabled={cursors.length === 1}
                onClick={() => setCursors(cursors.slice(0, -1))}
                className="underline disabled:opacity-40"
              >
                Previous photos
              </button>
              <button
                disabled={!photos.data.next_cursor}
                onClick={() =>
                  setCursors([...cursors, photos.data!.next_cursor!])
                }
                className="underline disabled:opacity-40"
              >
                Next photos
              </button>
            </div>
          </>
        )}
      </section>
    </Popup>
  );
}
