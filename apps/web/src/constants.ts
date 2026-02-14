export const API_BASE_URL = import.meta.env.VITE_API_URL || "http://localhost:3000";

export const AREA_PIN_MAP: Record<string, string> = {
  "MG Road / GPO": "560001",
  Shivajinagar: "560002",
  Malleshwaram: "560003",
  Basavanagudi: "560004",
  "Frazer Town": "560005",
  Ulsoor: "560008",
  "Richmond Town": "560009",
  Sadashivanagar: "560010",
  Jayanagar: "560011",
  Rajajinagar: "560018",
  Vijayanagar: "560020",
  Banashankari: "560025",
  "BTM Layout": "560028",
  Koramangala: "560034",
  Indiranagar: "560038",
  Hebbal: "560041",
  Yeshwanthpur: "560050",
  Domlur: "560054",
  Chamrajpet: "560055",
  Whitefield: "560066",
  "JP Nagar": "560070",
  "Electronic City": "560078",
  Marathahalli: "560085",
  Bellandur: "560095",
  "HSR Layout": "560102",
  "Sarjapur Road": "560103",
};

export const PIN_AREA_MAP: Record<string, string> = Object.fromEntries(
  Object.entries(AREA_PIN_MAP).map(([area, pin]) => [pin, area]),
);
