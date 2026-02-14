import { API_BASE_URL } from "../constants";
import {
  Lettering,
  RevisitLink,
  LeaderboardEntry,
  CollectionSummary,
  ChallengeData,
  Comment,
} from "../types";

export const ADMIN_SESSION_KEY = "ttl_admin_token";
export const USER_SESSION_KEY = "ttl_user_token";

export interface AuthUser {
  id: string;
  email: string;
  display_name?: string | null;
  role: string;
  created_at: string;
}

export interface NotificationItem {
  id: string;
  type: string;
  title: string;
  body?: string | null;
  metadata: Record<string, unknown>;
  is_read: boolean;
  created_at: string;
}

export interface AdminCommentItem {
  id: string;
  lettering_id: string;
  content: string;
  commenter_name?: string | null;
  commenter_email?: string | null;
  status: "VISIBLE" | "HIDDEN";
  moderation_score: number;
  moderation_flags: string[];
  auto_flagged: boolean;
  needs_review: boolean;
  review_priority: number;
  moderated_by?: string | null;
  moderation_reason?: string | null;
  created_at: string;
  updated_at: string;
  pin_code: string;
  contributor_tag: string;
  lettering_image_url: string;
  lettering_thumbnail: string;
}

export interface RegionPolicyItem {
  country_code: string;
  uploads_enabled: boolean;
  comments_enabled: boolean;
  discoverability_enabled: boolean;
  auto_moderation_level: "relaxed" | "standard" | "strict";
  created_at: string;
  updated_at: string;
}

export interface MyUploadItem {
  id: string;
  image_url: string;
  thumbnail_small: string;
  pin_code: string;
  contributor_tag: string;
  detected_text?: string | null;
  description?: string | null;
  status: string;
  likes_count: number;
  comments_count: number;
  report_count: number;
  moderation_reason?: string | null;
  moderated_at?: string | null;
  moderated_by?: string | null;
  created_at: string;
  updated_at: string;
}

export interface MyUploadStatusHistoryItem {
  id: string;
  from_status?: string | null;
  to_status: string;
  reason?: string | null;
  actor_type: string;
  actor_sub?: string | null;
  created_at: string;
}

export interface MyUploadMetadataHistoryItem {
  id: string;
  field_name: "description" | "contributor_tag" | "pin_code";
  old_value?: string | null;
  new_value?: string | null;
  created_at: string;
}

export interface MyUploadTimelineResponse {
  status_history: MyUploadStatusHistoryItem[];
  metadata_history: MyUploadMetadataHistoryItem[];
}

export interface AdminAuditLogItem {
  id: string;
  admin_sub: string;
  action: string;
  lettering_id?: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
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
  const res = await fetch(url, init);
  
  if (!res.ok) {
    if (res.status === 401 && authStorageKey) {
      sessionStorage.removeItem(authStorageKey);
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
  if (!text) {
    return undefined as T; 
  }

  try {
    return JSON.parse(text);
  } catch (e) {
    console.error("Malformed JSON:", text, e);
    throw new Error("Received invalid data from server");
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
  getUserToken() {
    return sessionStorage.getItem(USER_SESSION_KEY);
  },

  setUserToken(token: string) {
    sessionStorage.setItem(USER_SESSION_KEY, token);
  },

  clearUserToken() {
    sessionStorage.removeItem(USER_SESSION_KEY);
  },

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
  async upload(formData: FormData) {
    const res = await fetch(`${API_BASE_URL}/api/v1/letterings/upload`, {
      method: "POST",
      headers: getAuthHeaders(USER_SESSION_KEY),
      body: formData,
    });
    if (!res.ok) throw new Error(await res.text());
    return res.json();
  },

  // Like
  async toggleLike(id: string | number) {
    return fetchJson<{ liked: boolean; likes_count: number }>(
      `${API_BASE_URL}/api/v1/letterings/${id}/like`,
      { method: "POST" },
    );
  },

  // Search
  async search(q: string, lang?: string): Promise<Lettering[]> {
    const url = new URL(`${API_BASE_URL}/api/v1/letterings/search`);
    url.searchParams.set("q", q);
    if (lang?.trim()) {
      url.searchParams.set("lang", lang.trim());
    }
    const data = await fetchJson<Lettering[] | { letterings: Lettering[] }>(
      url.toString(),
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
    payload: { revisit_lettering_id: string; notes?: string }
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
      USER_SESSION_KEY
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
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ reason }),
      },
    );
  },

  // Comments
  async getComments(id: string | number): Promise<Comment[]> {
    return fetchJson<Comment[]>(
      `${API_BASE_URL}/api/v1/letterings/${id}/comments`,
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
    >(url.toString());
  },

  async searchCities(query: string, limit = 30) {
    return this.getCities({ q: query, limit, discover: true });
  },

  // Geo
  async getMarkers(params?: { cityId?: string | null; limit?: number }) {
    const url = new URL(`${API_BASE_URL}/api/v1/geo/markers`);
    if (params?.cityId) url.searchParams.set("city_id", params.cityId);
    if (params?.limit !== undefined) {
      url.searchParams.set("limit", String(params.limit));
    }
    return fetchJson<
      Array<{ id: string; lat: number; lng: number; thumbnail: string }>
    >(url.toString());
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
  }) {
    const url = new URL(`${API_BASE_URL}/api/v1/geo/coverage`);
    if (params?.cityId) url.searchParams.set("city_id", params.cityId);
    if (params?.minCount !== undefined) {
      url.searchParams.set("min_count", String(params.minCount));
    }
    if (params?.limit !== undefined) {
      url.searchParams.set("limit", String(params.limit));
    }

    return fetchJson<
      Array<{
        pin_code: string;
        city_id: string;
        city_name: string;
        lat: number;
        lng: number;
        count: number;
      }>
    >(url.toString());
  },

  // Community
  async getLeaderboard(): Promise<LeaderboardEntry[]> {
    return fetchJson<LeaderboardEntry[]>(
      `${API_BASE_URL}/api/v1/community/leaderboard`,
    );
  },

  // List all collections (public)
  async getCollections(): Promise<CollectionSummary[]> {
    return fetchJson<CollectionSummary[]>(`${API_BASE_URL}/api/v1/collections`);
  },

  // Get single collection detail
  async getCollection(id: string): Promise<any> {
    return fetchJson<any>(`${API_BASE_URL}/api/v1/collections/${id}`);
  },

  // Add lettering to a collection
  async addToCollection(collectionId: string, letteringId: string): Promise<void> {
    await fetchJson<void>(
      `${API_BASE_URL}/api/v1/collections/${collectionId}/items/${letteringId}`,
      {
        method: "POST",
        headers: getAuthHeaders(USER_SESSION_KEY),
      },
      USER_SESSION_KEY
    );
  },

  async getChallenges(): Promise<ChallengeData[]> {
    return fetchJson<ChallengeData[]>(`${API_BASE_URL}/api/v1/challenges`);
  },

  async createCollection(data: {
    name: string;
    description?: string;
    creator_tag: string;
  }) {
    return fetchJson<CollectionSummary>(`${API_BASE_URL}/api/v1/collections`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(data),
    });
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
    sessionStorage.setItem(USER_SESSION_KEY, data.token);
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
    sessionStorage.setItem(USER_SESSION_KEY, data.token);
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

    return fetchJson<{ items: Lettering[]; total: number }>(
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
};