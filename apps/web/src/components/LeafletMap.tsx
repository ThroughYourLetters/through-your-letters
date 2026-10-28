import React, { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  MapContainer,
  TileLayer,
  Marker,
  Popup,
  Circle,
  useMap,
} from "react-leaflet";
import L from "leaflet";
import markerIconUrl from "leaflet/dist/images/marker-icon.png";
import markerRetinaUrl from "leaflet/dist/images/marker-icon-2x.png";
import markerShadowUrl from "leaflet/dist/images/marker-shadow.png";
import { Loader2, Navigation, Eye, EyeOff } from "lucide-react";
import { Link } from "react-router-dom";
import { api } from "../lib/api";
import { webLimits } from "../lib/runtimeConfig";
import { groupMarkers, type MapMarker } from "../lib/mapMarkers";
import { useCityStore } from "../store/useCityStore";

const smallIcon = new L.Icon({
  iconUrl: markerIconUrl,
  iconRetinaUrl: markerRetinaUrl,
  shadowUrl: markerShadowUrl,
  iconSize: [20, 33],
  iconAnchor: [10, 33],
  popupAnchor: [0, -33],
  shadowSize: [33, 33],
});

function MapViewport({
  center,
  zoom,
  groups,
}: {
  center: [number, number];
  zoom: number;
  groups: MapMarker[][];
}) {
  const map = useMap();
  const [lat, lng] = center;
  // Preserve manual panning while data refreshes.
  const fitted = useRef(false);
  useEffect(() => {
    if (fitted.current) return;
    const positions = groups.map(
      (group) => [group[0].lat, group[0].lng] as [number, number],
    );
    if (positions.length) {
      fitted.current = true;
      map.fitBounds(L.latLngBounds(positions), {
        padding: [30, 30],
        maxZoom: 15,
      });
    } else map.setView([lat, lng], zoom);
  }, [map, lat, lng, zoom, groups]);
  return null;
}

function UserLocation({ position }: { position: [number, number] }) {
  const map = useMap();
  // Recenter after every successful location request.
  useEffect(() => {
    map.flyTo(position, 15, { duration: 1.5 });
  }, [map, position]);
  return (
    <Circle
      center={position}
      radius={200}
      pathOptions={{ color: "#3b82f6", fillOpacity: 0.3 }}
    />
  );
}

const coverageColor = (count: number) => {
  if (count <= 2) return "#ef4444";
  if (count <= 5) return "#f97316";
  if (count <= 10) return "#f59e0b";
  if (count <= 20) return "#22c55e";
  return "#16a34a";
};
const coverageRadius = (count: number) => {
  if (count <= 2) return 500;
  if (count <= 5) return 700;
  if (count <= 10) return 900;
  if (count <= 20) return 1100;
  return 1300;
};

const LeafletMap: React.FC<{ center?: [number, number]; zoom?: number }> = ({
  center = [12.9716, 77.5946],
  zoom = 12,
}) => {
  const { selectedCityId } = useCityStore();
  const [userLocation, setUserLocation] = useState<[number, number] | null>(
    null,
  );
  const locationRequest = useRef(0);
  const [locationError, setLocationError] = useState<string | null>(null);
  const [showCoverage, setShowCoverage] = useState(false);
  const markers = useQuery({
    queryKey: ["map", "markers", selectedCityId],
    queryFn: ({ signal }) =>
      api.getMarkers({ cityId: selectedCityId, limit: 5000, signal }),
  });
  const coverage = useQuery({
    queryKey: ["map", "coverage", selectedCityId],
    queryFn: ({ signal }) =>
      api.getCoverage({ cityId: selectedCityId, limit: 5000, signal }),
    enabled: showCoverage,
  });
  const markerGroups = useMemo(
    () => groupMarkers(markers.data ?? []),
    [markers.data],
  );
  const coverageGroups = useMemo(
    () => groupMarkers(coverage.data ?? []),
    [coverage.data],
  );

  useEffect(() => {
    const requests = locationRequest;
    setUserLocation(null);
    setLocationError(null);
    return () => {
      requests.current++;
    };
  }, [selectedCityId]);

  const handleNearMe = () => {
    const request = ++locationRequest.current;
    setLocationError(null);
    if (!navigator.geolocation) {
      setLocationError("Location is unavailable in this browser.");
      return;
    }
    navigator.geolocation.getCurrentPosition(
      (pos) => {
        if (request === locationRequest.current)
          setUserLocation([pos.coords.latitude, pos.coords.longitude]);
      },
      (error) => {
        if (request !== locationRequest.current) return;
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

  return (
    <div className="relative isolate">
      <div className="absolute top-4 right-4 z-[1000] flex flex-col gap-2">
        <button
          onClick={handleNearMe}
          className="bg-surface border border-line p-2 hover:bg-ink hover:text-white shadow-md"
          title="Near me"
        >
          <Navigation size={18} />
        </button>
        <button
          onClick={() => setShowCoverage((value) => !value)}
          aria-pressed={showCoverage}
          className="bg-surface border border-line p-2 hover:bg-ink hover:text-white shadow-md"
          title="Show coverage"
        >
          {showCoverage ? <EyeOff size={18} /> : <Eye size={18} />}
        </button>
      </div>
      {markers.isLoading && (
        <div
          role="status"
          className="absolute inset-0 z-[1000] bg-white/80 flex items-center justify-center gap-2"
        >
          <Loader2 size={24} className="animate-spin text-terracotta" />
          Loading map locations...
        </div>
      )}
      {markers.isError && (
        <div role="alert" className="p-4 text-red-700">
          Could not load map locations.{" "}
          <button className="underline" onClick={() => void markers.refetch()}>
            Retry map
          </button>
        </div>
      )}
      {!markers.isLoading && !markers.isError && markerGroups.length === 0 && (
        <p className="p-4">No mapped uploads found.</p>
      )}
      {showCoverage && coverage.isLoading && (
        <p role="status" className="p-4">
          Loading coverage...
        </p>
      )}
      {showCoverage && coverage.isError && (
        <div role="alert" className="p-4 text-red-700">
          Could not load coverage.{" "}
          <button className="underline" onClick={() => void coverage.refetch()}>
            Retry coverage
          </button>
        </div>
      )}
      {locationError && (
        <p role="alert" className="p-4 text-red-700">
          {locationError}
        </p>
      )}
      <MapContainer
        center={center}
        zoom={zoom}
        className="w-full h-[500px] md:h-[600px] border border-line"
        scrollWheelZoom
      >
        <TileLayer
          attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OSM</a>'
          url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
        />
        <MapViewport
          key={selectedCityId ?? "all"}
          center={center}
          zoom={zoom}
          groups={markerGroups}
        />
        {markerGroups.map((group) => {
          const first = group[0];
          const title = `${group.length} ${group.length === 1 ? "upload" : "uploads"} at this pin`;
          const icon =
            group.length === 1
              ? smallIcon
              : L.divIcon({
                  className: "map-upload-count",
                  html: `<span class="grid place-items-center bg-terracotta-dark text-white border-2 border-white rounded-full w-9 h-9 font-bold">${group.length}</span>`,
                  iconSize: [36, 36],
                  iconAnchor: [18, 18],
                });
          return (
            <Marker
              key={first.id}
              position={[first.lat, first.lng]}
              icon={icon}
              title={title}
              alt={title}
            >
              <Popup maxWidth={180} autoPanPadding={[35, 35]}>
                <p>{title}</p>
                <div className="max-h-64 overflow-y-auto space-y-3">
                  {group.map((marker, index) => (
                    <Link
                      key={marker.id}
                      to={`/lettering/${marker.id}`}
                      aria-label={`View upload ${index + 1} of ${group.length}`}
                      className="flex items-center gap-2"
                    >
                      {marker.thumbnail && (
                        <img
                          src={marker.thumbnail}
                          alt=""
                          loading="lazy"
                          className="w-12 h-12 object-cover"
                        />
                      )}
                      View upload {index + 1}
                    </Link>
                  ))}
                </div>
              </Popup>
            </Marker>
          );
        })}
        {showCoverage &&
          coverageGroups.map((group) => {
            const first = group[0];
            const count = group.reduce(
              (total, point) => total + point.count,
              0,
            );
            const color = coverageColor(count);
            return (
              <Circle
                key={`${first.city_id}-${first.pin_code}`}
                center={[first.lat, first.lng]}
                radius={coverageRadius(count)}
                pathOptions={{
                  color,
                  fillColor: color,
                  fillOpacity: 0.2,
                  weight: 2,
                }}
              >
                <Popup maxWidth={180} autoPanPadding={[35, 35]}>
                  <div className="max-h-64 overflow-y-auto">
                    {group.map((point) => (
                      <p key={`${point.city_id}-${point.pin_code}`}>
                        {point.city_name} {point.pin_code}: {point.count}{" "}
                        uploads
                      </p>
                    ))}
                  </div>
                </Popup>
              </Circle>
            );
          })}
        {userLocation && <UserLocation position={userLocation} />}
      </MapContainer>
    </div>
  );
};
export default LeafletMap;
