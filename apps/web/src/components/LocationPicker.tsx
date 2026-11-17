import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { divIcon, latLng, type LatLngTuple } from "leaflet";
import {
  MapContainer,
  Marker,
  TileLayer,
  useMap,
  useMapEvents,
} from "react-leaflet";
import type { ChosenLocation, PlaceSuggestion } from "@ttl/types";
import { api } from "../lib/api";
import { mapLimits, webLimits } from "../lib/runtimeConfig";
import ModalDialog from "./ModalDialog";
import "leaflet/dist/leaflet.css";

const markerIcon = divIcon({
  className: "",
  html: '<svg viewBox="0 0 24 32" width="24" height="32" aria-hidden="true"><path fill="#b44b36" stroke="white" stroke-width="2" d="M12 31C10 26 1 18 1 12a11 11 0 0 1 22 0c0 6-9 14-11 19Z"/><circle cx="12" cy="12" r="4" fill="white"/></svg>',
  iconSize: [24, 32],
  iconAnchor: [12, 32],
});

function PickerMap({
  center,
  onPick,
  onMove,
}: {
  center: LatLngTuple;
  onPick: (point: LatLngTuple) => void;
  onMove: (point: LatLngTuple) => void;
}) {
  const map = useMap();
  useEffect(() => {
    map.invalidateSize();
    map.setView(center, Math.max(map.getZoom(), mapLimits.detail_zoom));
    const point = map.getCenter().wrap();
    onMove([point.lat, point.lng]);
  }, [map, center, onMove]);
  useMapEvents({
    click: (event) => {
      const point = event.latlng.wrap();
      onPick([point.lat, point.lng]);
    },
    move: () => {
      const point = map.getCenter().wrap();
      onMove([point.lat, point.lng]);
    },
  });
  return null;
}

export default function LocationPicker({
  value,
  center,
  onConfirm,
  onClose,
}: {
  value: ChosenLocation | null;
  center: LatLngTuple;
  onConfirm: (point: ChosenLocation) => void;
  onClose: () => void;
}) {
  const id = useId();
  const [point, setPoint] = useState<ChosenLocation | null>(value);
  const [viewCenter, setViewCenter] = useState<LatLngTuple>(() =>
    value ? [value.latitude, value.longitude] : center,
  );
  const mapCenter = useRef(viewCenter);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<PlaceSuggestion[]>([]);
  const [searching, setSearching] = useState(false);
  const [locating, setLocating] = useState(false);
  const [error, setError] = useState("");
  const errorRef = useRef<HTMLParagraphElement>(null);
  const searchInput = useRef<HTMLInputElement>(null);
  const search = useRef<AbortController | null>(null);
  const gps = useRef<AbortController | null>(null);
  // A ref callback keeps map movement independent of React's render cycle.
  const moved = useRef((next: LatLngTuple) => {
    mapCenter.current = next;
  }).current;

  useEffect(() => {
    searchInput.current?.focus();
    return () => {
      search.current?.abort();
      search.current = null;
      gps.current?.abort();
      gps.current = null;
    };
  }, []);
  useEffect(() => {
    if (error) errorRef.current?.focus();
  }, [error]);

  const cancelGps = () => {
    gps.current?.abort();
    gps.current = null;
    setLocating(false);
  };
  const pick = ([latitude, longitude]: LatLngTuple) => {
    cancelGps();
    setError("");
    setPoint((previous) => ({
      latitude,
      longitude,
      place_name: previous?.place_name || "Selected map point",
      accuracy_m: previous?.accuracy_m ?? null,
    }));
  };
  const searchPlaces = async () => {
    search.current?.abort();
    cancelGps();
    const controller = new AbortController();
    search.current = controller;
    setSearching(true);
    setResults([]);
    setError("");
    try {
      const places = await api.searchPlaces(query.trim(), {
        signal: controller.signal,
      });
      if (search.current !== controller || controller.signal.aborted) return;
      setResults(places);
      if (!places.length)
        setError(
          "No places found. Try another name or select a point on the map.",
        );
    } catch (failure) {
      if (search.current === controller && !controller.signal.aborted)
        setError(
          failure instanceof Error
            ? failure.message
            : "Place search is unavailable. You can still use the map.",
        );
    } finally {
      if (search.current === controller) {
        search.current = null;
        setSearching(false);
      }
    }
  };
  const locate = () => {
    cancelGps();
    setError("");
    if (!navigator.geolocation) {
      setError(
        "This browser cannot find your location. Search for a place or use the map.",
      );
      return;
    }
    const controller = new AbortController();
    gps.current = controller;
    setLocating(true);
    navigator.geolocation.getCurrentPosition(
      (position) => {
        if (gps.current !== controller || controller.signal.aborted) return;
        gps.current = null;
        setLocating(false);
        setPoint(null);
        setViewCenter([position.coords.latitude, position.coords.longitude]);
      },
      (failure) => {
        if (gps.current !== controller || controller.signal.aborted) return;
        gps.current = null;
        setLocating(false);
        setError(
          failure.code === 1
            ? "Location access denied. Search for a place or use the map."
            : "Your location could not be found. Search for a place or use the map.",
        );
      },
      { timeout: webLimits.geolocation_timeout_seconds * 1000 },
    );
  };

  return createPortal(
    <ModalDialog
      titleId={`${id}-title`}
      onClose={onClose}
      className="w-[calc(100%-2rem)] max-w-2xl max-h-[90dvh] overflow-y-auto bg-paper p-4 sm:p-6 space-y-4"
    >
      <div className="flex items-start justify-between gap-3">
        <h2 id={`${id}-title`} className="font-display text-2xl">
          Choose photo location
        </h2>
        <button type="button" className="btn-outline" onClick={onClose}>
          Cancel
        </button>
      </div>
      <p className="text-sm text-muted">
        The confirmed point and place name will be public. Choose where the
        photo was taken.
      </p>
      {error && (
        <p
          ref={errorRef}
          tabIndex={-1}
          role="alert"
          className="text-sm text-red-700"
        >
          {error}
        </p>
      )}
      <form
        aria-label="Search places"
        className="space-y-2"
        onSubmit={(event) => {
          event.preventDefault();
          event.stopPropagation();
          void searchPlaces();
        }}
      >
        <label htmlFor={`${id}-search`} className="eyebrow">
          Search for a place
        </label>
        <input
          id={`${id}-search`}
          ref={searchInput}
          className="field"
          value={query}
          required
          minLength={2}
          maxLength={200}
          onChange={(event) => {
            search.current?.abort();
            search.current = null;
            setSearching(false);
            setResults([]);
            setQuery(event.target.value);
          }}
        />
        <button type="submit" className="btn-dark" disabled={searching}>
          {searching ? "Searching…" : "Search place"}
        </button>
      </form>
      {results.length > 0 && (
        <ul aria-label="Place results" className="space-y-2">
          {results.map((place, index) => (
            <li key={`${place.latitude}:${place.longitude}:${index}`}>
              <button
                type="button"
                className="btn-outline w-full !text-left break-words"
                onClick={() => {
                  cancelGps();
                  setPoint({
                    latitude: place.latitude,
                    longitude: place.longitude,
                    place_name: place.name,
                    accuracy_m: null,
                  });
                  setViewCenter([place.latitude, place.longitude]);
                  setResults([]);
                  setError("");
                }}
              >
                {place.name}
              </button>
            </li>
          ))}
        </ul>
      )}
      <button
        type="button"
        className="btn-outline"
        disabled={locating}
        onClick={locate}
      >
        {locating ? "Finding location…" : "Use current location"}
      </button>
      <p className="text-sm text-muted">
        Device location only moves the map. Select a point and confirm it. You
        can also pan with the arrow keys and use the map centre.
      </p>
      <MapContainer
        center={viewCenter}
        zoom={mapLimits.detail_zoom}
        minZoom={2}
        maxZoom={19}
        maxBounds={[
          [-85.05112878, -180],
          [85.05112878, 180],
        ]}
        maxBoundsViscosity={1}
        className="h-64 w-full"
        aria-label="Location selection map"
      >
        <TileLayer
          url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
          maxZoom={19}
          attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
        />
        <PickerMap center={viewCenter} onPick={pick} onMove={moved} />
        {point && (
          <Marker
            icon={markerIcon}
            position={[point.latitude, point.longitude]}
            draggable
            eventHandlers={{
              dragend: (event) => {
                const position = latLng(event.target.getLatLng()).wrap();
                pick([position.lat, position.lng]);
              },
            }}
          />
        )}
      </MapContainer>
      <button
        type="button"
        className="btn-outline"
        onClick={() => pick(mapCenter.current)}
      >
        Use map centre
      </button>
      <form
        aria-label="Confirm photo location"
        className="space-y-3"
        onSubmit={(event) => {
          event.preventDefault();
          event.stopPropagation();
          if (!point || !point.place_name.trim()) return;
          onConfirm({ ...point, place_name: point.place_name.trim() });
        }}
      >
        {point && (
          <>
            <p role="status" className="text-sm">
              Selected point: {point.latitude.toFixed(5)},{" "}
              {point.longitude.toFixed(5)}
            </p>
            <label htmlFor={`${id}-name`} className="eyebrow">
              Place name
            </label>
            <input
              id={`${id}-name`}
              className="field"
              value={point.place_name}
              required
              maxLength={200}
              onChange={(event) =>
                setPoint({ ...point, place_name: event.target.value })
              }
            />
            <label htmlFor={`${id}-accuracy`} className="eyebrow">
              Estimated accuracy
            </label>
            <select
              id={`${id}-accuracy`}
              className="field"
              value={point.accuracy_m ?? ""}
              onChange={(event) =>
                setPoint({
                  ...point,
                  accuracy_m: event.target.value
                    ? Number(event.target.value)
                    : null,
                })
              }
            >
              <option value="">Unknown</option>
              {[10, 50, 100, 500, 1000].map((metres) => (
                <option key={metres} value={metres}>
                  Within about {metres} metres
                </option>
              ))}
              {point.accuracy_m !== null &&
                ![10, 50, 100, 500, 1000].includes(point.accuracy_m) && (
                  <option value={point.accuracy_m}>
                    Within about {point.accuracy_m} metres
                  </option>
                )}
            </select>
          </>
        )}
        <button
          type="submit"
          className="btn-dark w-full"
          disabled={!point || !point.place_name.trim()}
        >
          Confirm location
        </button>
      </form>
    </ModalDialog>,
    document.body,
  );
}
