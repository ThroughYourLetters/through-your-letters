import type {
  AuthUser,
  NotificationItem,
  AdminCommentItem,
  MyUploadItem,
  MyUploadTimelineResponse,
  AdminAuditLogItem,
  ModerationItem,
  UploadResponse,
} from "@ttl/types";
export type {
  AuthUser,
  NotificationItem,
  AdminCommentItem,
  MyUploadItem,
  MyUploadStatusHistoryItem,
  MyUploadMetadataHistoryItem,
  MyUploadTimelineResponse,
  AdminAuditLogItem,
} from "@ttl/types";
import type {
  MapBounds,
  MapFilters,
  DensityResponse,
  MapPhoto,
} from "../types/mapDensity";
import type {
  LocationRecord,
  RepairEvidence,
  RepairPreview,
  AppliedRepair,
} from "../types/locationRepairs";
import { API_BASE_URL } from "../constants";
import {
  ADMIN_SESSION_KEY,
  USER_SESSION_KEY,
  captureUserSession,
  isCurrentUserSession,
  replaceUserSession,
  sessionChanged,
} from "./userSession";
export { ADMIN_SESSION_KEY, USER_SESSION_KEY } from "./userSession";

// Registered by useAuthStore at startup. Avoids circular import between api.ts ↔ useAuthStore.ts.
let _onUnauthorized: (() => void) | null = null;
export function registerUnauthorizedHandler(fn: () => void) {
  _onUnauthorized = fn;
}
import {
  Lettering,
  RevisitLink,
  LeaderboardEntry,
  CollectionSummary,
  ChallengeData,
  Comment,
} from "../types";

export interface RegionPolicyItem {
  country_code: string;
  uploads_enabled: boolean;
  comments_enabled: boolean;
  discoverability_enabled: boolean;
  auto_moderation_level: "relaxed" | "standard" | "strict";
  created_at: string;
  updated_at: string;
}

function getAuthHeaders(storageKey: string): HeadersInit {
  const token = sessionStorage.getItem(storageKey);
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function fetchJson<T>(
  url: string,
  init?: RequestInit,
  authStorageKey?: string,
): Promise<T> {
  const session =
    authStorageKey === USER_SESSION_KEY ? captureUserSession() : null;
  const controller = new AbortController();
  const abort = () => controller.abort();
  const signals = [init?.signal, session?.signal].filter(
    (signal): signal is AbortSignal => !!signal,
  );
  for (const signal of signals) {
    if (signal.aborted) abort();
    signal.addEventListener("abort", abort, { once: true });
  }
  try {
    const res = await fetch(url, {
      ...init,
      signal: controller.signal,
      cache: "no-store",
    });
    if (session && !isCurrentUserSession(session)) throw sessionChanged();

    if (!res.ok) {
      if (res.status === 401 && authStorageKey) {
        if (authStorageKey === USER_SESSION_KEY) {
          if (_onUnauthorized) _onUnauthorized();
          else replaceUserSession(null, null);
        } else sessionStorage.removeItem(authStorageKey);
      }
      const text = await res.text().catch(() => "");
      let message = `HTTP ${res.status}`;
      if (text) {
        try {
          const json = JSON.parse(text);
          message = json.error || json.message || message;
        } catch {
          message = text;
        }
      }
      throw new Error(message);
    }

    if (res.status === 204) return undefined as T;

    const text = await res.text();
    if (session && !isCurrentUserSession(session)) throw sessionChanged();
    if (!text) {
      return undefined as T;
    }

    try {
      return JSON.parse(text);
    } catch (e) {
      console.error("Malformed API JSON", e);
      throw new Error("Received invalid data from server");
    }
  } finally {
    for (const signal of signals) signal.removeEventListener("abort", abort);
  }
}

export interface GalleryParams {
  limit?: number;
  offset?: number;
  cityId?: string | null;
  script?: string | null;
  style?: string | null;
  sortBy?: string | null;
}

export interface GalleryResponse {
  letterings: Lettering[];
  total: number;
}

export const api = {
  // Gallery
  async getGallery(params: GalleryParams = {}): Promise<GalleryResponse> {
    const { limit = 50, offset = 0, cityId, script, style, sortBy } = params;
    const url = new URL(`${API_BASE_URL}/api/v1/letterings`);
    url.searchParams.set("limit", String(limit));
    url.searchParams.set("offset", String(offset));
    if (cityId) url.searchParams.set("city_id", cityId);
    if (script) url.searchParams.set("script", script);
    if (style) url.searchParams.set("style", style);
    if (sortBy) url.searchParams.set("sort_by", sortBy);
    return fetchJson<GalleryResponse>(url.toString());
  },

  // Single lettering
  async getLettering(id: string | number): Promise<Lettering> {
    return fetchJson<Lettering>(
      `${API_BASE_URL}/api/v1/letterings/${id}`,
      { headers: getAuthHeaders(USER_SESSION_KEY) },
      USER_SESSION_KEY,
    );
  },

  // Upload (includes user auth if present)
  async upload(formData: FormData, requestKey?: string) {
    return fetchJson<UploadResponse>(
      `${API_BASE_URL}/api/v1/letterings/upload`,
      {
        method: "POST",
        headers: {
          ...getAuthHeaders(USER_SESSION_KEY),
          ...(requestKey ? { "Idempotency-Key": requestKey } : {}),
        },
        body: formData,
      },
      USER_SESSION_KEY,
    );
  },

  // Like
  async toggleLike(id: string | number) {
    return fetchJson<{ liked: boolean; likes_count: number }>(
      `${API_BASE_URL}/api/v1/letterings/${id}/like`,
      { method: "POST", headers: getAuthHeaders(USER_SESSION_KEY) },
      USER_SESSION_KEY,
    );
  },

  // Search
  async search(
    q: string,
    lang?: string,
    signal?: AbortSignal,
  ): Promise<Lettering[]> {
    const url = new URL(`${API_BASE_URL}/api/v1/letterings/search`);
    url.searchParams.set("q", q);
    if (lang?.trim()) {
      url.searchParams.set("lang", lang.trim());
    }
    const data = await fetchJson<Lettering[] | { letterings: Lettering[] }>(
      url.toString(),
      { signal },
    );
    return Array.isArray(data) ? data : data.letterings;
  },

  async deleteOwnLettering(id: string | number) {
    await fetchJson<void>(
      `${API_BASE_URL}/api/v1/letterings/${id}`,
      {
        method: "DELETE",
        headers: getAuthHeaders(USER_SESSION_KEY),
      },
      USER_SESSION_KEY,
    );
    return true;
  },

  // Contributor
  async getContributor(tag: string, limit = 50, offset = 0) {
    return fetchJson<{
      contributor_tag: string;
      total_count: number;
      letterings: Lettering[];
    }>(
      `${API_BASE_URL}/api/v1/contributors/${encodeURIComponent(tag)}?limit=${limit}&offset=${offset}`,
    );
  },

  // Revisits
  async getRevisits(id: string | number): Promise<{ revisits: RevisitLink[] }> {
    return fetchJson<{ revisits: RevisitLink[] }>(
      `${API_BASE_URL}/api/v1/letterings/${id}/revisits`,
    );
  },

  // Link a revisit (Location Timeline)
  async linkRevisit(
    originalId: string,
    payload: { revisit_lettering_id: string; notes?: string },
  ): Promise<void> {
    await fetchJson<void>(
      `${API_BASE_URL}/api/v1/letterings/${originalId}/revisits`,
      {
        method: "POST",
        headers: {
          ...getAuthHeaders(USER_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify(payload),
      },
      USER_SESSION_KEY,
    );
  },

  // Similar
  async getSimilar(id: string | number) {
    return fetchJson<{
      similar: Array<{
        id: string;
        thumbnail?: string;
        image_url: string;
        detected_text?: string;
        ml_style?: string;
        ml_script?: string;
      }>;
    }>(`${API_BASE_URL}/api/v1/letterings/${id}/similar`);
  },

  // Report
  async reportLettering(id: string | number, reason: string) {
    return fetchJson<{ message: string }>(
      `${API_BASE_URL}/api/v1/letterings/${id}/report`,
      {
        method: "POST",
        headers: {
          ...getAuthHeaders(USER_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ reason }),
      },
      USER_SESSION_KEY,
    );
  },

  // Comments
  async getComments(id: string | number): Promise<Comment[]> {
    return fetchJson<Comment[]>(
      `${API_BASE_URL}/api/v1/letterings/${id}/comments`,
      { headers: getAuthHeaders(USER_SESSION_KEY) },
      USER_SESSION_KEY,
    );
  },

  async addComment(id: string | number, content: string): Promise<Comment> {
    return fetchJson<Comment>(
      `${API_BASE_URL}/api/v1/letterings/${id}/comments`,
      {
        method: "POST",
        headers: {
          ...getAuthHeaders(USER_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ content }),
      },
      USER_SESSION_KEY,
    );
  },

  // Cities
  async getCities(params?: {
    q?: string;
    countryCode?: string;
    limit?: number;
    offset?: number;
    discover?: boolean;
    signal?: AbortSignal;
  }) {
    const url = new URL(`${API_BASE_URL}/api/v1/cities`);
    if (params?.q) url.searchParams.set("q", params.q);
    if (params?.countryCode) {
      url.searchParams.set("country_code", params.countryCode);
    }
    if (params?.limit !== undefined) {
      url.searchParams.set("limit", String(params.limit));
    }
    if (params?.offset !== undefined) {
      url.searchParams.set("offset", String(params.offset));
    }
    if (params?.discover !== undefined) {
      url.searchParams.set("discover", String(params.discover));
    }

    return fetchJson<
      Array<{
        id: string;
        name: string;
        country_code: string;
        center_lat: number | null;
        center_lng: number | null;
        default_zoom: number | null;
        description?: string | null;
        cover_image_url?: string | null;
        is_active: boolean | null;
      }>
    >(url.toString(), { signal: params?.signal });
  },

  async searchCities(
    query: string,
    limit = 30,
    options?: { discover?: boolean; signal?: AbortSignal },
  ) {
    return this.getCities({ q: query, limit, ...options });
  },

  async getDensity(
    { west, south, east, north }: MapBounds,
    zoom: number,
    filters: MapFilters,
    signal?: AbortSignal,
  ) {
    const url = new URL(`${API_BASE_URL}/api/v1/geo/density`);
    for (const [key, value] of Object.entries({
      west,
      south,
      east,
      north,
      zoom,
      ...filters,
    }))
      if (value !== undefined) url.searchParams.set(key, String(value));
    const data = await fetchJson<DensityResponse>(url.toString(), { signal });
    if (
      !data ||
      !Array.isArray(data.cells) ||
      typeof data.truncated !== "boolean" ||
      data.cells.some(
        (cell) =>
          !cell ||
          typeof cell.id !== "string" ||
          typeof cell.thumbnail !== "string" ||
          !Number.isSafeInteger(cell.count) ||
          cell.count < 1 ||
          !validCoordinate(cell) ||
          !validBounds(cell.bounds),
      )
    )
      throw new Error("Invalid map density response");
    return data;
  },
  async getMapSummary(filters: MapFilters, signal?: AbortSignal) {
    const url = new URL(`${API_BASE_URL}/api/v1/geo/summary`);
    for (const [key, value] of Object.entries(filters))
      if (value !== undefined) url.searchParams.set(key, value);
    const data = await fetchJson<{ unmapped_count: number }>(url.toString(), {
      signal,
    });
    if (
      !data ||
      !Number.isSafeInteger(data.unmapped_count) ||
      data.unmapped_count < 0
    )
      throw new Error("Invalid map summary response");
    return data;
  },
  async getMapPhotos(
    bounds: MapBounds,
    filters: MapFilters,
    after?: string,
    signal?: AbortSignal,
  ) {
    const url = new URL(`${API_BASE_URL}/api/v1/geo/photos`);
    for (const [key, value] of Object.entries({ ...bounds, ...filters, after }))
      if (value !== undefined) url.searchParams.set(key, String(value));
    const data = await fetchJson<{
      items: MapPhoto[];
      next_cursor: string | null;
    }>(url.toString(), { signal });
    if (
      !data ||
      !Array.isArray(data.items) ||
      (data.next_cursor !== null && typeof data.next_cursor !== "string") ||
      data.items.some(
        (photo) =>
          !photo ||
          typeof photo.id !== "string" ||
          typeof photo.thumbnail !== "string" ||
          typeof photo.pin_code !== "string" ||
          !validCoordinate(photo),
      )
    )
      throw new Error("Invalid hotspot photos response");
    return data;
  },

  // Geo
  async getMarkers(params?: {
    cityId?: string | null;
    limit?: number;
    signal?: AbortSignal;
  }) {
    const url = new URL(`${API_BASE_URL}/api/v1/geo/markers`);
    if (params?.cityId) url.searchParams.set("city_id", params.cityId);
    if (params?.limit !== undefined) {
      url.searchParams.set("limit", String(params.limit));
    }
    const data = await fetchJson<
      Array<{
        id: string;
        lat: number;
        lng: number;
        thumbnail: string;
        location_source?: "postal_code" | "legacy_unverified";
      }>
    >(url.toString(), { signal: params?.signal });
    if (
      !Array.isArray(data) ||
      data.some(
        (marker) =>
          !marker ||
          typeof marker.id !== "string" ||
          !marker.id ||
          typeof marker.thumbnail !== "string" ||
          !Number.isFinite(marker.lat) ||
          !Number.isFinite(marker.lng) ||
          Math.abs(marker.lat) > 90 ||
          Math.abs(marker.lng) > 180,
      )
    ) {
      throw new Error("Invalid map locations response");
    }
    return data;
  },

  async getNeighborhoods() {
    return fetchJson<{
      neighborhoods: Array<{ pin_code: string; count: number }>;
    }>(`${API_BASE_URL}/api/v1/analytics/neighborhoods`);
  },

  async getCoverage(params?: {
    cityId?: string | null;
    minCount?: number;
    limit?: number;
    signal?: AbortSignal;
  }) {
    const url = new URL(`${API_BASE_URL}/api/v1/geo/coverage`);
    if (params?.cityId) url.searchParams.set("city_id", params.cityId);
    if (params?.minCount !== undefined) {
      url.searchParams.set("min_count", String(params.minCount));
    }
    if (params?.limit !== undefined) {
      url.searchParams.set("limit", String(params.limit));
    }

    const data = await fetchJson<
      Array<{
        pin_code: string;
        city_id: string;
        city_name: string;
        lat: number;
        lng: number;
        count: number;
      }>
    >(url.toString(), { signal: params?.signal });
    if (
      !Array.isArray(data) ||
      data.some(
        (point) =>
          !point ||
          typeof point.city_id !== "string" ||
          typeof point.city_name !== "string" ||
          typeof point.pin_code !== "string" ||
          !Number.isFinite(point.lat) ||
          !Number.isFinite(point.lng) ||
          Math.abs(point.lat) > 90 ||
          Math.abs(point.lng) > 180 ||
          !Number.isSafeInteger(point.count) ||
          point.count < 1,
      )
    ) {
      throw new Error("Invalid coverage response");
    }
    return data;
  },

  // Community
  async getLeaderboard(): Promise<LeaderboardEntry[]> {
    return fetchJson<LeaderboardEntry[]>(
      `${API_BASE_URL}/api/v1/community/leaderboard`,
    );
  },

  // List all collections (public), paginated
  async getCollections(params?: { limit?: number; offset?: number }): Promise<{
    items: CollectionSummary[];
    total: number;
    limit: number;
    offset: number;
  }> {
    const url = new URL(`${API_BASE_URL}/api/v1/collections`);
    if (params?.limit !== undefined)
      url.searchParams.set("limit", String(params.limit));
    if (params?.offset !== undefined)
      url.searchParams.set("offset", String(params.offset));
    return fetchJson(url.toString());
  },

  // Get single collection detail with paginated items
  async getCollection(
    id: string,
    params?: { limit?: number; offset?: number },
  ): Promise<{
    id: string;
    name: string;
    description?: string | null;
    creator_tag: string;
    is_public: boolean;
    created_at: string;
    total_items: number;
    limit: number;
    offset: number;
    items: Array<{
      id: string;
      image_url: string;
      thumbnail: string;
      detected_text?: string | null;
      contributor_tag: string;
    }>;
  }> {
    const url = new URL(`${API_BASE_URL}/api/v1/collections/${id}`);
    if (params?.limit !== undefined)
      url.searchParams.set("limit", String(params.limit));
    if (params?.offset !== undefined)
      url.searchParams.set("offset", String(params.offset));
    return fetchJson(
      url.toString(),
      { headers: getAuthHeaders(USER_SESSION_KEY) },
      USER_SESSION_KEY,
    );
  },

  // Add lettering to a collection
  async addToCollection(
    collectionId: string,
    letteringId: string,
  ): Promise<void> {
    await fetchJson<void>(
      `${API_BASE_URL}/api/v1/collections/${collectionId}/items/${letteringId}`,
      {
        method: "POST",
        headers: getAuthHeaders(USER_SESSION_KEY),
      },
      USER_SESSION_KEY,
    );
  },

  async removeFromCollection(
    collectionId: string,
    letteringId: string,
  ): Promise<void> {
    await fetchJson<void>(
      `${API_BASE_URL}/api/v1/collections/${collectionId}/items/${letteringId}`,
      {
        method: "DELETE",
        headers: getAuthHeaders(USER_SESSION_KEY),
      },
      USER_SESSION_KEY,
    );
  },

  async getChallenges(): Promise<ChallengeData[]> {
    return fetchJson<ChallengeData[]>(`${API_BASE_URL}/api/v1/challenges`);
  },

  async createCollection(data: { name: string; description?: string }) {
    return fetchJson<CollectionSummary>(
      `${API_BASE_URL}/api/v1/collections`,
      {
        method: "POST",
        headers: {
          ...getAuthHeaders(USER_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify(data),
      },
      USER_SESSION_KEY,
    );
  },

  // User auth
  async registerUser(payload: {
    email: string;
    password: string;
    display_name?: string;
  }) {
    const data = await fetchJson<{ token: string; user: AuthUser }>(
      `${API_BASE_URL}/api/v1/auth/register`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      },
    );
    return data;
  },

  async loginUser(payload: { email: string; password: string }) {
    const data = await fetchJson<{ token: string; user: AuthUser }>(
      `${API_BASE_URL}/api/v1/auth/login`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      },
    );
    return data;
  },

  async getCurrentUser() {
    return fetchJson<AuthUser>(
      `${API_BASE_URL}/api/v1/auth/me`,
      { headers: getAuthHeaders(USER_SESSION_KEY) },
      USER_SESSION_KEY,
    );
  },

  // Me workspace
  async getMyUploads(params?: {
    limit?: number;
    offset?: number;
    status?: string;
  }) {
    const url = new URL(`${API_BASE_URL}/api/v1/me/letterings`);
    if (params?.limit) url.searchParams.set("limit", String(params.limit));
    if (params?.offset) url.searchParams.set("offset", String(params.offset));
    if (params?.status) url.searchParams.set("status", params.status);
    return fetchJson<{
      items: MyUploadItem[];
      total: number;
      limit: number;
      offset: number;
    }>(
      url.toString(),
      { headers: getAuthHeaders(USER_SESSION_KEY) },
      USER_SESSION_KEY,
    );
  },

  async updateMyUpload(
    id: string,
    payload: {
      description?: string;
      contributor_tag?: string;
      pin_code?: string;
    },
  ) {
    return fetchJson<MyUploadItem>(
      `${API_BASE_URL}/api/v1/me/letterings/${id}`,
      {
        method: "PATCH",
        headers: {
          ...getAuthHeaders(USER_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify(payload),
      },
      USER_SESSION_KEY,
    );
  },

  async getNotifications(params?: { limit?: number; offset?: number }) {
    const url = new URL(`${API_BASE_URL}/api/v1/me/notifications`);
    if (params?.limit) url.searchParams.set("limit", String(params.limit));
    if (params?.offset) url.searchParams.set("offset", String(params.offset));
    return fetchJson<{
      items: NotificationItem[];
      total: number;
      unread: number;
      limit: number;
      offset: number;
    }>(
      url.toString(),
      { headers: getAuthHeaders(USER_SESSION_KEY) },
      USER_SESSION_KEY,
    );
  },

  async markNotificationRead(id: string) {
    return fetchJson<void>(
      `${API_BASE_URL}/api/v1/me/notifications/${id}/read`,
      {
        method: "POST",
        headers: getAuthHeaders(USER_SESSION_KEY),
      },
      USER_SESSION_KEY,
    );
  },

  // Admin
  async adminLogin(email: string, password: string) {
    const data = await fetchJson<{ token: string }>(
      `${API_BASE_URL}/api/v1/admin/login`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, password }),
      },
    );
    sessionStorage.setItem(ADMIN_SESSION_KEY, data.token);
    return data;
  },

  async adminGetQueue(params: {
    status: string;
    limit?: number;
    offset?: number;
  }) {
    const url = new URL(`${API_BASE_URL}/api/v1/admin/moderation`);
    url.searchParams.set("status", params.status);
    if (params.limit !== undefined) {
      url.searchParams.set("limit", String(params.limit));
    }
    if (params.offset !== undefined) {
      url.searchParams.set("offset", String(params.offset));
    }

    return fetchJson<{ items: ModerationItem[]; total: number }>(
      url.toString(),
      { headers: getAuthHeaders(ADMIN_SESSION_KEY) },
      ADMIN_SESSION_KEY,
    );
  },

  async adminBulkLetterings(payload: {
    ids: string[];
    action: "approve" | "reject" | "delete" | "keep";
    reason?: string;
  }) {
    return fetchJson<{
      requested: number;
      processed: number;
      failed: number;
      failed_items: Array<{ id: string; error: string }>;
    }>(
      `${API_BASE_URL}/api/v1/admin/letterings/bulk`,
      {
        method: "POST",
        headers: {
          ...getAuthHeaders(ADMIN_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify(payload),
      },
      ADMIN_SESSION_KEY,
    );
  },

  async adminApprove(id: string) {
    return fetchJson<void>(
      `${API_BASE_URL}/api/v1/admin/letterings/${id}/approve`,
      { method: "POST", headers: getAuthHeaders(ADMIN_SESSION_KEY) },
      ADMIN_SESSION_KEY,
    );
  },

  async adminReject(id: string, reason: string) {
    return fetchJson<void>(
      `${API_BASE_URL}/api/v1/admin/letterings/${id}/reject`,
      {
        method: "POST",
        headers: {
          ...getAuthHeaders(ADMIN_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ reason }),
      },
      ADMIN_SESSION_KEY,
    );
  },

  async adminDelete(id: string) {
    return fetchJson<void>(
      `${API_BASE_URL}/api/v1/admin/letterings/${id}`,
      { method: "DELETE", headers: getAuthHeaders(ADMIN_SESSION_KEY) },
      ADMIN_SESSION_KEY,
    );
  },

  async adminClearReports(id: string) {
    return fetchJson<void>(
      `${API_BASE_URL}/api/v1/admin/letterings/${id}/clear-reports`,
      { method: "POST", headers: getAuthHeaders(ADMIN_SESSION_KEY) },
      ADMIN_SESSION_KEY,
    );
  },

  async getLocationRepairs(
    after?: string,
    signal?: AbortSignal,
    includeResolved = false,
  ) {
    const url = new URL(`${API_BASE_URL}/api/v1/admin/location-repairs`);
    if (includeResolved) url.searchParams.set("include_resolved", "true");
    if (after) url.searchParams.set("after", after);
    return fetchJson<{ items: LocationRecord[]; next_cursor: string | null }>(
      url.toString(),
      { headers: getAuthHeaders(ADMIN_SESSION_KEY), signal },
      ADMIN_SESSION_KEY,
    );
  },
  async previewLocationRepair(id: string, evidence: RepairEvidence) {
    return fetchJson<RepairPreview>(
      `${API_BASE_URL}/api/v1/admin/location-repairs/${id}/preview`,
      {
        method: "POST",
        headers: {
          ...getAuthHeaders(ADMIN_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify(evidence),
      },
      ADMIN_SESSION_KEY,
    );
  },
  async applyLocationRepair(preview: RepairPreview) {
    return fetchJson<AppliedRepair>(
      `${API_BASE_URL}/api/v1/admin/location-repairs/${preview.current.id}/apply`,
      {
        method: "POST",
        headers: {
          ...getAuthHeaders(ADMIN_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          repair_id: preview.repair_id,
          expected_updated_at: preview.current.updated_at,
          evidence: preview.evidence,
          reviewed: true,
        }),
      },
      ADMIN_SESSION_KEY,
    );
  },
  async getLocationHistory(id: string, after?: string, signal?: AbortSignal) {
    const url = new URL(
      `${API_BASE_URL}/api/v1/admin/location-repairs/${id}/history`,
    );
    if (after) url.searchParams.set("after", after);
    return fetchJson<{ items: AppliedRepair[]; next_cursor: string | null }>(
      url.toString(),
      { headers: getAuthHeaders(ADMIN_SESSION_KEY), signal },
      ADMIN_SESSION_KEY,
    );
  },

  async adminGetStats() {
    return fetchJson<{
      total_uploads: number;
      pending_approvals: number;
      approved: number;
      rejected: number;
      total_cities: number;
      total_likes: number;
      total_comments: number;
    }>(
      `${API_BASE_URL}/api/v1/admin/stats`,
      { headers: getAuthHeaders(ADMIN_SESSION_KEY) },
      ADMIN_SESSION_KEY,
    );
  },

  async adminDiscoverCities(payload: {
    query: string;
    country_code?: string;
    limit?: number;
  }) {
    return fetchJson<{
      processed: number;
      upserted: number;
      failed: number;
    }>(
      `${API_BASE_URL}/api/v1/admin/cities/discover`,
      {
        method: "POST",
        headers: {
          ...getAuthHeaders(ADMIN_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify(payload),
      },
      ADMIN_SESSION_KEY,
    );
  },

  async adminBootstrapCapitals(payload?: { limit?: number }) {
    return fetchJson<{
      processed: number;
      upserted: number;
      failed: number;
    }>(
      `${API_BASE_URL}/api/v1/admin/cities/bootstrap-capitals`,
      {
        method: "POST",
        headers: {
          ...getAuthHeaders(ADMIN_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify(payload || {}),
      },
      ADMIN_SESSION_KEY,
    );
  },

  async adminGetComments(params?: {
    status?: "ALL" | "VISIBLE" | "HIDDEN";
    limit?: number;
    offset?: number;
    q?: string;
    needs_review?: boolean;
    min_score?: number;
    sort?: "priority" | "newest" | "score";
  }) {
    const url = new URL(`${API_BASE_URL}/api/v1/admin/comments`);
    if (params?.status) url.searchParams.set("status", params.status);
    if (params?.limit !== undefined)
      url.searchParams.set("limit", String(params.limit));
    if (params?.offset !== undefined)
      url.searchParams.set("offset", String(params.offset));
    if (params?.q) url.searchParams.set("q", params.q);
    if (params?.needs_review !== undefined) {
      url.searchParams.set("needs_review", String(params.needs_review));
    }
    if (params?.min_score !== undefined) {
      url.searchParams.set("min_score", String(params.min_score));
    }
    if (params?.sort) {
      url.searchParams.set("sort", params.sort);
    }

    return fetchJson<{
      items: AdminCommentItem[];
      total: number;
      limit: number;
      offset: number;
    }>(
      url.toString(),
      { headers: getAuthHeaders(ADMIN_SESSION_KEY) },
      ADMIN_SESSION_KEY,
    );
  },

  async adminHideComment(id: string, reason?: string) {
    return fetchJson<void>(
      `${API_BASE_URL}/api/v1/admin/comments/${id}/hide`,
      {
        method: "POST",
        headers: {
          ...getAuthHeaders(ADMIN_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ reason }),
      },
      ADMIN_SESSION_KEY,
    );
  },

  async adminRestoreComment(id: string) {
    return fetchJson<void>(
      `${API_BASE_URL}/api/v1/admin/comments/${id}/restore`,
      {
        method: "POST",
        headers: getAuthHeaders(ADMIN_SESSION_KEY),
      },
      ADMIN_SESSION_KEY,
    );
  },

  async adminDeleteComment(id: string) {
    return fetchJson<void>(
      `${API_BASE_URL}/api/v1/admin/comments/${id}`,
      {
        method: "DELETE",
        headers: getAuthHeaders(ADMIN_SESSION_KEY),
      },
      ADMIN_SESSION_KEY,
    );
  },

  async adminBulkComments(payload: {
    ids: string[];
    action: "hide" | "restore" | "delete";
    reason?: string;
  }) {
    return fetchJson<{
      requested: number;
      processed: number;
      failed: number;
      failed_items: Array<{ id: string; error: string }>;
    }>(
      `${API_BASE_URL}/api/v1/admin/comments/bulk`,
      {
        method: "POST",
        headers: {
          ...getAuthHeaders(ADMIN_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify(payload),
      },
      ADMIN_SESSION_KEY,
    );
  },

  async adminGetRegionPolicies(params?: {
    countryCode?: string;
    limit?: number;
    offset?: number;
  }) {
    const url = new URL(`${API_BASE_URL}/api/v1/admin/region-policies`);
    if (params?.countryCode) {
      url.searchParams.set("country_code", params.countryCode.toUpperCase());
    }
    if (params?.limit !== undefined) {
      url.searchParams.set("limit", String(params.limit));
    }
    if (params?.offset !== undefined) {
      url.searchParams.set("offset", String(params.offset));
    }

    return fetchJson<{
      items: RegionPolicyItem[];
      total: number;
      limit: number;
      offset: number;
    }>(
      url.toString(),
      { headers: getAuthHeaders(ADMIN_SESSION_KEY) },
      ADMIN_SESSION_KEY,
    );
  },

  async adminUpsertRegionPolicy(
    countryCode: string,
    payload: {
      uploads_enabled?: boolean;
      comments_enabled?: boolean;
      discoverability_enabled?: boolean;
      auto_moderation_level?: "relaxed" | "standard" | "strict";
    },
  ) {
    return fetchJson<RegionPolicyItem>(
      `${API_BASE_URL}/api/v1/admin/region-policies/${countryCode.toUpperCase()}`,
      {
        method: "PUT",
        headers: {
          ...getAuthHeaders(ADMIN_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify(payload),
      },
      ADMIN_SESSION_KEY,
    );
  },

  async adminGetAuditLogs(params?: {
    action?: string;
    countryCode?: string;
    letteringId?: string;
    limit?: number;
    offset?: number;
  }) {
    const url = new URL(`${API_BASE_URL}/api/v1/admin/audit-logs`);
    if (params?.action) url.searchParams.set("action", params.action);
    if (params?.countryCode) {
      url.searchParams.set("country_code", params.countryCode.toUpperCase());
    }
    if (params?.letteringId)
      url.searchParams.set("lettering_id", params.letteringId);
    if (params?.limit !== undefined) {
      url.searchParams.set("limit", String(params.limit));
    }
    if (params?.offset !== undefined) {
      url.searchParams.set("offset", String(params.offset));
    }

    return fetchJson<{
      items: AdminAuditLogItem[];
      total: number;
      limit: number;
      offset: number;
    }>(
      url.toString(),
      { headers: getAuthHeaders(ADMIN_SESSION_KEY) },
      ADMIN_SESSION_KEY,
    );
  },

  async getMyUploadTimeline(id: string) {
    return fetchJson<MyUploadTimelineResponse>(
      `${API_BASE_URL}/api/v1/me/letterings/${id}/timeline`,
      { headers: getAuthHeaders(USER_SESSION_KEY) },
      USER_SESSION_KEY,
    );
  },

  async deleteAccount(password: string): Promise<void> {
    await fetchJson<void>(
      `${API_BASE_URL}/api/v1/me/account`,
      {
        method: "DELETE",
        headers: {
          ...getAuthHeaders(USER_SESSION_KEY),
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ password }),
      },
      USER_SESSION_KEY,
    );
    if (_onUnauthorized) _onUnauthorized();
    else replaceUserSession(null, null);
  },
};

function validCoordinate(point: { lat: number; lng: number }) {
  return (
    Number.isFinite(point.lat) &&
    Number.isFinite(point.lng) &&
    Math.abs(point.lat) <= 90 &&
    Math.abs(point.lng) <= 180
  );
}
function validBounds(bounds: MapBounds) {
  return (
    bounds &&
    validCoordinate({ lat: bounds.south, lng: bounds.west }) &&
    validCoordinate({ lat: bounds.north, lng: bounds.east }) &&
    bounds.south < bounds.north &&
    bounds.west !== bounds.east
  );
}
