export interface ZinePageData {
  id: string | number;
  title: string;
  location: string;
  culturalContext: string;
  historicalNote: string;
  image: string;
  thumbnail?: string;
  imageSource: string;
  sourceUrl: string;
  vibe: string;
  readMoreUrl: string;
  isUserContribution?: boolean;
  contributorName?: string;
  description?: string;
  report_count?: number;
  report_reasons?: string[];
  likes_count?: number;
  comments_count?: number;
  ml_script?: string;
  is_owner?: boolean;
}

export interface Comment {
  id: string;
  lettering_id: string;
  content: string;
  user_id?: string | null;
  commenter_name?: string | null;
  status?: "VISIBLE" | "HIDDEN";
  moderation_score?: number;
  moderation_flags?: string[];
  auto_flagged?: boolean;
  needs_review?: boolean;
  review_priority?: number;
  moderated_by?: string | null;
  moderation_reason?: string | null;
  updated_at?: string;
  created_at: string;
}

export interface RevisitLink {
  id: string;
  original_lettering_id: string;
  revisit_lettering_id: string;
  notes?: string;
  created_at: string;
  original: {
    image_url: string;
    created_at: string;
  };
  revisit: {
    image_url: string;
    created_at: string;
  };
}

export interface LeaderboardEntry {
  tag: string;
  count: number;
  total_likes: number;
}

export interface CollectionSummary {
  id: string;
  name: string;
  description?: string;
  creator_tag: string;
  cover_image_url?: string;
  item_count: number;
  created_at: string;
}

export interface ChallengeData {
  id: string;
  title: string;
  description?: string;
  target_script?: string;
  target_area?: string;
  target_count: number;
  current_count: number;
  status: string;
  ends_at?: string;
}

export interface Lettering {
  id: string;
  image_url: string;
  thumbnail_urls: {
    small: string;
    medium: string;
    large: string;
  };
  location: {
    type: string;
    coordinates: [number, number];
  };
  pin_code: string;
  contributor_tag: string;
  detected_text?: string;
  description?: string;
  ml_metadata?: {
    style?: string;
    script?: string;
  };
  cultural_context?: string;
  status: "PENDING" | "APPROVED" | "REJECTED" | "REPORTED";
  created_at: string;
  likes_count?: number;
  comments_count?: number;
  report_count?: number;
  report_reasons?: string[];
  is_owner?: boolean;
}

export interface NeighborhoodCount {
  pin_code: string;
  count: number;
}

export interface GalleryFilters {
  script?: string;
  style?: string;
  sortBy?: string;
}

export const SCRIPT_OPTIONS = [
  "Kannada",
  "Devanagari",
  "Latin",
  "Tamil",
  "Telugu",
  "Bengali",
  "Malayalam",
  "Urdu",
  "Odia",
  "Gujarati",
] as const;

export const STYLE_OPTIONS = [
  "Hand-painted",
  "Neon",
  "Carved",
  "Stenciled",
  "Printed",
  "Digital",
  "Calligraphic",
  "Graffiti",
] as const;

export const SORT_OPTIONS = [
  { value: "newest", label: "Newest" },
  { value: "oldest", label: "Oldest" },
  { value: "popular", label: "Most Liked" },
] as const;
