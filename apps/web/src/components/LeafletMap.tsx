import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  MapContainer,
  TileLayer,
  Marker,
  Circle,
  useMap,
  useMapEvents,
} from "react-leaflet";
import L from "leaflet";
import { Navigation } from "lucide-react";
import { useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import { mapLimits, webLimits } from "../lib/runtimeConfig";
import { useCityStore } from "../store/useCityStore";
import HeatLayer, { nearestLongitude } from "./HeatLayer";
import MapHotspot from "./MapHotspot";
import type {
  DensityCell,
  DensityResponse,
  MapBounds,
} from "../types/mapDensity";

const EMPTY: DensityCell[] = [];
const MAX_LAT = 85.0511287798066;
type Viewport = MapBounds & { zoom: number; center_lng: number };
const wrap = (lng: number) => ((((lng + 180) % 360) + 360) % 360) - 180;

function ViewportObserver({
  onChange,
  cells,
  onSelect,
}: {
  onChange: (view: Viewport) => void;
  cells: DensityCell[];
  onSelect: (cell: DensityCell, longitude?: number) => void;
}) {
  const map = useMap();
  const timer = useRef<ReturnType<typeof setTimeout>>();
  const update = useCallback(() => {
    const size = map.getSize();
    if (size.x <= 0 || size.y <= 0) return;
    const padding = mapLimits.heat_radius_pixels + mapLimits.heat_blur_pixels;
    const bounds = L.latLngBounds(
      map.containerPointToLatLng([-padding, -padding]),
      map.containerPointToLatLng([size.x + padding, size.y + padding]),
    );
    const west = bounds.getWest(),
      east = bounds.getEast();
    const south = Math.max(-MAX_LAT, Math.min(MAX_LAT, bounds.getSouth()));
    const north = Math.max(-MAX_LAT, Math.min(MAX_LAT, bounds.getNorth()));
    if (south >= north) return;
    const full = east - west >= 360;
    onChange({
      west: full ? -180 : wrap(west),
      east: full ? 180 : wrap(east),
      south,
      north,
      zoom: Math.round(map.getZoom()),
      center_lng: map.getCenter().lng,
    });
  }, [map, onChange]);
  const schedule = useCallback(() => {
    clearTimeout(timer.current);
    timer.current = setTimeout(update, mapLimits.move_debounce_ms);
  }, [update]);
  useMapEvents({
    moveend: schedule,
    resize: schedule,
    click: (event) => {
      // Only background taps select heat cells.
      const target = event.originalEvent.target;
      if (
        !(target instanceof Element) ||
        (target !== map.getContainer() &&
          !target.closest(".leaflet-tile-pane,.leaflet-overlay-pane"))
      )
        return;
      let nearest: DensityCell | undefined,
        distance = mapLimits.heat_radius_pixels + mapLimits.heat_blur_pixels;
      for (const cell of cells) {
        // Match the tapped world copy exactly.
        const point = map.latLngToContainerPoint([
          cell.lat,
          nearestLongitude(cell.lng, event.latlng.lng),
        ]);
        const current = point.distanceTo(event.containerPoint);
        if (current < distance) {
          nearest = cell;
          distance = current;
        }
      }
      if (nearest) onSelect(nearest, event.latlng.lng);
    },
  });
  useEffect(() => {
    map.invalidateSize({ pan: false });
    update();
    return () => clearTimeout(timer.current);
  }, [map, update]);
  return null;
}
function UserLocation({ position }: { position: [number, number] }) {
  const map = useMap();
  useEffect(() => {
    map.flyTo(position, mapLimits.detail_zoom, {
      animate: !window.matchMedia("(prefers-reduced-motion: reduce)").matches,
    });
  }, [map, position]);
  return (
    <Circle
      center={position}
      radius={200}
      pathOptions={{ color: "#3b82f6", fillOpacity: 0.3 }}
    />
  );
}

function PhotoMarker({
  cell,
  longitude,
  onSelect,
}: {
  cell: DensityCell;
  longitude: number;
  onSelect: (cell: DensityCell) => void;
}) {
  const icon = useMemo(() => {
    const root = document.createElement("div");
    root.className =
      "relative h-11 w-11 rounded-2xl border-[3px] border-white bg-white shadow-lg";
    const fallback = document.createElement("span");
    fallback.className =
      "absolute inset-0 grid place-items-center rounded-xl bg-ink text-white text-xs font-semibold";
    fallback.textContent = String(cell.count);
    root.append(fallback);
    if (cell.thumbnail) {
      const image = document.createElement("img");
      image.src = cell.thumbnail;
      image.alt = "";
      image.loading = "lazy";
      image.className = "relative h-full w-full rounded-xl object-cover";
      image.onerror = () => image.remove();
      root.append(image);
    }
    if (cell.count > 1) {
      const badge = document.createElement("span");
      badge.className =
        "absolute -right-2 -top-2 rounded-full bg-ink px-1.5 py-0.5 text-[10px] font-bold text-white border border-white";
      badge.textContent = String(cell.count);
      root.append(badge);
    }
    return L.divIcon({
      className: "map-photo-marker",
      html: root,
      iconSize: [44, 44],
      iconAnchor: [22, 22],
    });
  }, [cell.count, cell.thumbnail]);
  const title = `${cell.count} ${cell.count === 1 ? "upload" : "uploads"} in this area`;
  return (
    <Marker
      position={[cell.lat, nearestLongitude(cell.lng, longitude)]}
      icon={icon}
      title={title}
      alt={title}
      eventHandlers={{
        click: () => onSelect(cell),
        keydown: (event) => {
          if (
            event.originalEvent.key === "Enter" ||
            event.originalEvent.key === " "
          ) {
            event.originalEvent.preventDefault();
            event.originalEvent.stopPropagation();
            onSelect(cell);
          }
        },
      }}
    />
  );
}

export default function LeafletMap({
  center = [20, 0],
  zoom = 2,
}: {
  center?: [number, number];
  zoom?: number;
}) {
  const { selectedCityId } = useCityStore();
  const [params] = useSearchParams();
  const script = params.get("script") || undefined,
    style = params.get("style") || undefined;
  const filters = useMemo(
    () => ({ city_id: selectedCityId || undefined, script, style }),
    [selectedCityId, script, style],
  );
  const [view, setView] = useState<Viewport>();
  const [selected, setSelected] = useState<{
    cell: DensityCell;
    sequence: number;
    position: [number, number];
  } | null>(null);
  const selectionSequence = useRef(0);
  const [areaPage, setAreaPage] = useState(0);
  const [areasOpen, setAreasOpen] = useState(false);
  const [userLocation, setUserLocation] = useState<[number, number] | null>(
    null,
  );
  const [locationError, setLocationError] = useState<string | null>(null);
  const [locating, setLocating] = useState(false);
  const locationRequest = useRef(0);
  const filterKey = JSON.stringify(filters);
  const density = useQuery<DensityResponse>({
    queryKey: ["map", "density", filterKey, view],
    placeholderData: (previous, query) =>
      query?.queryKey[2] === filterKey ? previous : undefined,
    enabled: !!view,
    queryFn: ({ signal }) => api.getDensity(view!, view!.zoom, filters, signal),
    gcTime: mapLimits.query_gc_seconds * 1000,
  });
  const summary = useQuery({
    queryKey: ["map", "summary", filters],
    queryFn: ({ signal }) => api.getMapSummary(filters, signal),
  });
  const cells = density.data?.cells ?? EMPTY;
  const select = useCallback(
    (cell: DensityCell, longitude = view?.center_lng ?? center[1]) =>
      setSelected({
        cell,
        sequence: ++selectionSequence.current,
        position: [cell.lat, nearestLongitude(cell.lng, longitude)],
      }),
    [view?.center_lng, center],
  );
  useEffect(() => {
    setSelected(null);
    setAreaPage(0);
  }, [filters]);
  useEffect(() => {
    setAreaPage(0);
  }, [cells]);
  useEffect(() => {
    const requests = locationRequest;
    return () => {
      requests.current++;
    };
  }, []);
  const nearMe = () => {
    const request = ++locationRequest.current;
    setLocationError(null);
    if (!navigator.geolocation) {
      setLocationError("Location is unavailable in this browser.");
      return;
    }
    setLocating(true);
    navigator.geolocation.getCurrentPosition(
      (pos) => {
        if (request !== locationRequest.current) return;
        setLocating(false);
        if (
          !Number.isFinite(pos.coords.latitude) ||
          !Number.isFinite(pos.coords.longitude) ||
          Math.abs(pos.coords.latitude) > 90 ||
          Math.abs(pos.coords.longitude) > 180
        ) {
          setLocationError("Your location is unavailable. Try again.");
          return;
        }
        setUserLocation([pos.coords.latitude, pos.coords.longitude]);
      },
      (error) => {
        if (request !== locationRequest.current) return;
        setLocating(false);
        setLocationError(
          error.code === error.PERMISSION_DENIED
            ? "Location permission was denied. Check your browser settings."
            : error.code === error.TIMEOUT
              ? "Location request timed out. Try again."
              : "Your location is unavailable. Try again.",
        );
      },
      { timeout: webLimits.geolocation_timeout_seconds * 1000 },
    );
  };
  const pageSize = webLimits.gallery_page_size;
  return (
    <div className="space-y-3">
      {density.isError && (
        <p role="alert">
          Could not load map locations.{" "}
          <button className="underline" onClick={() => void density.refetch()}>
            Retry map
          </button>
        </p>
      )}
      {density.data?.truncated && (
        <p role="status">Zoom in to see all areas.</p>
      )}
      {density.data && cells.length === 0 && (
        <p>No mapped uploads found in this view.</p>
      )}
      {locationError && <p role="alert">{locationError}</p>}
      <div className="relative isolate rounded-2xl overflow-hidden">
        {density.isFetching && (
          <div
            role="status"
            className="absolute top-3 right-3 z-[1000] rounded-full bg-white/95 px-3 py-2 text-sm shadow-soft"
          >
            Loading map locations...
          </div>
        )}
        <button
          title="Near me"
          aria-label="Near me"
          disabled={locating}
          onClick={nearMe}
          className="absolute bottom-8 right-3 z-[1000] grid h-11 w-11 place-items-center rounded-full bg-white shadow-lg border border-line"
        >
          <Navigation size={20} className={locating ? "animate-pulse" : ""} />
        </button>
        <div
          aria-label="Heatmap density legend"
          className="absolute bottom-8 left-3 z-[1000] rounded-full bg-white/95 px-3 py-2 text-xs shadow-soft"
        >
          <span className="flex items-center gap-2">
            Less{" "}
            <span
              className="inline-block h-2 w-20 rounded-full"
              style={{
                background:
                  "linear-gradient(to right,#164dff,#00cfff,#00de73,#ffe600,#ff2100)",
              }}
            />{" "}
            More
          </span>
        </div>
        <MapContainer
          center={center}
          zoom={zoom}
          minZoom={0}
          maxZoom={22}
          worldCopyJump
          zoomAnimation={
            !window.matchMedia("(prefers-reduced-motion: reduce)").matches
          }
          className="map-density-viewport w-full h-[calc(100dvh-18rem)] min-h-[240px] md:h-[72dvh] md:min-h-[500px]"
          scrollWheelZoom
        >
          <TileLayer
            attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
            url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
          />
          <ViewportObserver
            onChange={setView}
            cells={cells}
            onSelect={select}
          />
          <HeatLayer cells={cells} />
          {(view?.zoom ?? zoom) >= mapLimits.detail_zoom &&
            cells.map((cell) => (
              <PhotoMarker
                key={cell.id}
                cell={cell}
                longitude={view?.center_lng ?? center[1]}
                onSelect={select}
              />
            ))}
          {selected && (
            <MapHotspot
              key={selected.sequence}
              cell={selected.cell}
              filters={filters}
              position={selected.position}
              onClose={() =>
                setSelected((current) =>
                  current?.sequence === selected.sequence ? null : current,
                )
              }
            />
          )}
          {userLocation && <UserLocation position={userLocation} />}
        </MapContainer>
      </div>
      {summary.isError ? (
        <p role="alert">
          Could not load location status.{" "}
          <button className="underline" onClick={() => void summary.refetch()}>
            Retry location status
          </button>
        </p>
      ) : (
        summary.data &&
        summary.data.unmapped_count > 0 && (
          <p>
            {summary.data.unmapped_count} approved uploads are not shown because
            their locations need review.
          </p>
        )
      )}
      {cells.length > 0 && (
        <details open={areasOpen} className="space-y-2">
          <summary
            onClick={(event) => {
              event.preventDefault();
              setAreasOpen((value) => !value);
            }}
            className="cursor-pointer text-sm font-medium"
          >
            Browse visible areas
          </summary>
          <section aria-label="Areas in this view" className="space-y-2">
            <div className="flex flex-wrap gap-2">
              {cells
                .slice(areaPage * pageSize, (areaPage + 1) * pageSize)
                .map((cell, index) => (
                  <button
                    key={cell.id}
                    className="btn-secondary"
                    onClick={() => select(cell)}
                  >
                    Area {areaPage * pageSize + index + 1}: {cell.count}{" "}
                    {cell.count === 1 ? "upload" : "uploads"}
                  </button>
                ))}
            </div>
            {cells.length > pageSize && (
              <div className="flex gap-3">
                <button
                  disabled={areaPage === 0}
                  onClick={() => setAreaPage(areaPage - 1)}
                >
                  Previous areas
                </button>
                <button
                  disabled={(areaPage + 1) * pageSize >= cells.length}
                  onClick={() => setAreaPage(areaPage + 1)}
                >
                  Next areas
                </button>
              </div>
            )}
          </section>
        </details>
      )}
    </div>
  );
}
