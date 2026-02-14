import { create } from "zustand";

interface CityState {
  selectedCityId: string | null;
  selectedCityName: string;
  centerLat: number;
  centerLng: number;
  defaultZoom: number;
  setCity: (id: string | null, name: string, lat: number, lng: number, zoom: number) => void;
  clearCity: () => void;
}

export const useCityStore = create<CityState>((set) => ({
  selectedCityId: null,
  selectedCityName: "All Cities",
  centerLat: 12.9716,
  centerLng: 77.5946,
  defaultZoom: 12,
  setCity: (id, name, lat, lng, zoom) =>
    set({
      selectedCityId: id,
      selectedCityName: name,
      centerLat: lat,
      centerLng: lng,
      defaultZoom: zoom,
    }),
  clearCity: () =>
    set({
      selectedCityId: null,
      selectedCityName: "All Cities",
      centerLat: 12.9716,
      centerLng: 77.5946,
      defaultZoom: 12,
    }),
}));
