export interface MapBounds {
  west: number;
  south: number;
  east: number;
  north: number;
}
export interface MapFilters {
  city_id?: string;
  script?: string;
  style?: string;
}
export interface DensityCell {
  id: string;
  lat: number;
  lng: number;
  count: number;
  thumbnail: string;
  bounds: MapBounds;
}
export interface MapPhoto {
  id: string;
  thumbnail: string;
  pin_code: string;
  lat: number;
  lng: number;
}
export interface DensityResponse {
  cells: DensityCell[];
  truncated: boolean;
}
