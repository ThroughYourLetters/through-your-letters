import React, { useState, useEffect } from "react";
import {
  MapContainer,
  TileLayer,
  Marker,
  Popup,
  Circle,
  useMap,
} from "react-leaflet";
import L from "leaflet";
import { Loader2, Navigation, Eye, EyeOff } from "lucide-react";
import { Link } from "react-router-dom";
import { api } from "../lib/api";
import { useCityStore } from "../store/useCityStore";

// Fix Leaflet default icon path issue with bundlers
delete (L.Icon.Default.prototype as any)._getIconUrl;
L.Icon.Default.mergeOptions({
  iconRetinaUrl:
    "https://unpkg.com/leaflet@1.9.4/dist/images/marker-icon-2x.png",
  iconUrl: "https://unpkg.com/leaflet@1.9.4/dist/images/marker-icon.png",
  shadowUrl: "https://unpkg.com/leaflet@1.9.4/dist/images/marker-shadow.png",
});

interface MapMarker {
  id: string;
  lat: number;
  lng: number;
  thumbnail: string;
}

interface CoveragePoint {
  pin_code: string;
  city_id: string;
  city_name: string;
  lat: number;
  lng: number;
  count: number;
}

const smallIcon = new L.Icon({
  iconUrl: "https://unpkg.com/leaflet@1.9.4/dist/images/marker-icon.png",
  iconRetinaUrl:
    "https://unpkg.com/leaflet@1.9.4/dist/images/marker-icon-2x.png",
  shadowUrl: "https://unpkg.com/leaflet@1.9.4/dist/images/marker-shadow.png",
  iconSize: [20, 33],
  iconAnchor: [10, 33],
  popupAnchor: [0, -33],
  shadowSize: [33, 33],
});

function FlyToLocation({ lat, lng }: { lat: number; lng: number }) {
  const map = useMap();
  useEffect(() => {
    map.flyTo([lat, lng], 15, { duration: 1.5 });
  }, [lat, lng, map]);
  return null;
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

const LeafletMap: React.FC<{
  center?: [number, number];
  zoom?: number;
}> = ({ center = [12.9716, 77.5946], zoom = 12 }) => {
  const { selectedCityId } = useCityStore();
  const [markers, setMarkers] = useState<MapMarker[]>([]);
  const [loading, setLoading] = useState(true);
  const [userLocation, setUserLocation] = useState<[number, number] | null>(
    null,
  );
  const [showCoverage, setShowCoverage] = useState(false);
  const [coverageData, setCoverageData] = useState<CoveragePoint[]>([]);

  useEffect(() => {
    api
      .getMarkers({ cityId: selectedCityId, limit: 5000 })
      .then((data) => setMarkers(data))
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [selectedCityId]);

  useEffect(() => {
    setCoverageData([]);
  }, [selectedCityId]);

  useEffect(() => {
    if (showCoverage && coverageData.length === 0) {
      api
        .getCoverage({ cityId: selectedCityId, limit: 5000 })
        .then((data) => setCoverageData(data))
        .catch(() => {});
    }
  }, [showCoverage, coverageData.length, selectedCityId]);

  const handleNearMe = () => {
    if (!navigator.geolocation) return;
    navigator.geolocation.getCurrentPosition(
      (pos) => setUserLocation([pos.coords.latitude, pos.coords.longitude]),
      () => {},
    );
  };

  return (
    <div className="relative">
      <div className="absolute top-4 right-4 z-[1000] flex flex-col gap-2">
        <button
          onClick={handleNearMe}
          className="bg-white border-2 border-black p-2 hover:bg-black hover:text-white transition-colors shadow-md"
          title="Near Me"
        >
          <Navigation size={18} />
        </button>
        <button
          onClick={() => setShowCoverage(!showCoverage)}
          className={`bg-white border-2 border-black p-2 hover:bg-black hover:text-white transition-colors shadow-md ${showCoverage ? "bg-[#cc543a] text-white" : ""}`}
          title="Show coverage"
        >
          {showCoverage ? <EyeOff size={18} /> : <Eye size={18} />}
        </button>
      </div>

      {loading && (
        <div className="absolute inset-0 z-[1000] bg-white/80 flex items-center justify-center">
          <Loader2 size={32} className="animate-spin text-[#cc543a]" />
        </div>
      )}

      <MapContainer
        center={center}
        zoom={zoom}
        className="w-full h-[500px] md:h-[600px] border-4 border-black"
        scrollWheelZoom={true}
      >
        <TileLayer
          attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OSM</a>'
          url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
        />

        {userLocation && (
          <FlyToLocation lat={userLocation[0]} lng={userLocation[1]} />
        )}

        {markers.map((m) => (
          <Marker key={m.id} position={[m.lat, m.lng]} icon={smallIcon}>
            <Popup>
              <div className="text-center">
                {m.thumbnail && (
                  <img
                    src={m.thumbnail}
                    className="w-24 h-24 object-cover mx-auto mb-2 border border-black"
                    alt="Lettering"
                  />
                )}
                <Link
                  to={`/lettering/${m.id}`}
                  className="text-[10px] font-black uppercase text-[#cc543a] hover:underline"
                >
                  View in Archive
                </Link>
              </div>
            </Popup>
          </Marker>
        ))}

        {showCoverage &&
          coverageData.map((point) => {
            const color = coverageColor(point.count);
            return (
              <Circle
                key={`${point.city_id}-${point.pin_code}`}
                center={[point.lat, point.lng]}
                radius={coverageRadius(point.count)}
                pathOptions={{
                  color,
                  fillColor: color,
                  fillOpacity: 0.2,
                  weight: 2,
                }}
              >
                <Popup>
                  <span className="text-[10px] font-black">
                    {point.city_name} {point.pin_code}: {point.count} uploads
                  </span>
                </Popup>
              </Circle>
            );
          })}

        {userLocation && (
          <Circle
            center={userLocation}
            radius={200}
            pathOptions={{
              color: "#3b82f6",
              fillColor: "#3b82f6",
              fillOpacity: 0.3,
            }}
          />
        )}
      </MapContainer>
    </div>
  );
};

export default LeafletMap;
