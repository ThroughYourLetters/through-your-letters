export interface LocationRecord {
  id: string;
  city_name: string;
  country_code: string;
  pin_code: string;
  image_url: string;
  lat: number | null;
  lng: number | null;
  location_source: "postal_code" | "unresolved" | "legacy_unverified";
  location_error: string | null;
  updated_at: string;
}

export interface RepairEvidence {
  lat: number;
  lng: number;
  city_name: string;
  country_code: string;
  pin_code: string;
  source_name: string;
  source_url: string;
  reason: string;
}

export interface RepairPreview {
  repair_id: string;
  current: LocationRecord;
  evidence: RepairEvidence;
}

export interface AppliedRepair {
  id: string;
  lettering_id: string;
  reviewed_by: string;
  before_location: LocationRecord;
  after_location: LocationRecord;
  evidence: RepairEvidence;
  created_at: string;
}
