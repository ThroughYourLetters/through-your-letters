import { useState } from "react";
import { useToastStore } from "../../store/useToastStore";

export type QueueStatus = "PENDING" | "APPROVED" | "REJECTED";
export interface QueueFilters {
  statusFilter: QueueStatus;
  queueLimit: number;
  tab: "queue" | "reports";
}
export interface CommentFilters {
  commentStatusFilter: "ALL" | "VISIBLE" | "HIDDEN";
  commentQuery: string;
  reviewOnly: boolean;
  minScore: number;
  commentSort: "priority" | "newest" | "score";
  commentLimit: number;
}
interface PresetBase {
  id: string;
  name: string;
}
export type AdminPreset =
  | (PresetBase & { type: "queue"; payload: QueueFilters })
  | (PresetBase & { type: "comments"; payload: CommentFilters });
const KEY = "ttl_admin_filter_presets_v1";
const pageSize = (value: unknown): value is number =>
  typeof value === "number" && [10, 25, 50, 100].includes(value);
const record = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === "object";
function parse(value: unknown): AdminPreset | null {
  if (
    !record(value) ||
    typeof value.id !== "string" ||
    typeof value.name !== "string" ||
    value.name.trim().length === 0 ||
    value.name.length > 60 ||
    !record(value.payload)
  )
    return null;
  const p = value.payload;
  const base = { id: value.id, name: value.name };
  if (
    value.type === "queue" &&
    (p.tab === "queue" || p.tab === "reports") &&
    (p.statusFilter === "PENDING" ||
      p.statusFilter === "APPROVED" ||
      p.statusFilter === "REJECTED") &&
    pageSize(p.queueLimit)
  )
    return {
      ...base,
      type: "queue",
      payload: {
        tab: p.tab,
        statusFilter: p.statusFilter,
        queueLimit: p.queueLimit,
      },
    };
  if (
    value.type === "comments" &&
    (p.commentStatusFilter === "ALL" ||
      p.commentStatusFilter === "VISIBLE" ||
      p.commentStatusFilter === "HIDDEN") &&
    typeof p.commentQuery === "string" &&
    p.commentQuery.length <= 500 &&
    typeof p.reviewOnly === "boolean" &&
    typeof p.minScore === "number" &&
    Number.isInteger(p.minScore) &&
    p.minScore >= 0 &&
    p.minScore <= 100 &&
    (p.commentSort === "priority" ||
      p.commentSort === "newest" ||
      p.commentSort === "score") &&
    pageSize(p.commentLimit)
  )
    return {
      ...base,
      type: "comments",
      payload: {
        commentStatusFilter: p.commentStatusFilter,
        commentQuery: p.commentQuery,
        reviewOnly: p.reviewOnly,
        minScore: p.minScore,
        commentSort: p.commentSort,
        commentLimit: p.commentLimit,
      },
    };
  return null;
}
function read(): AdminPreset[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(KEY) || "[]");
    return Array.isArray(value)
      ? value
          .map(parse)
          .filter((item): item is AdminPreset => item !== null)
          .slice(0, 20)
      : [];
  } catch {
    return [];
  }
}
export function useAdminPresets() {
  const [presets, setPresets] = useState<AdminPreset[]>(read);
  const addToast = useToastStore((state) => state.addToast);
  function write(next: AdminPreset[]) {
    try {
      localStorage.setItem(KEY, JSON.stringify(next));
      setPresets(next);
      return true;
    } catch {
      addToast("Filters could not be saved on this device.", "error");
      return false;
    }
  }
  function save(
    value:
      | { type: "queue"; payload: QueueFilters }
      | { type: "comments"; payload: CommentFilters },
  ) {
    const name = window.prompt("Preset name:")?.trim();
    if (!name) return;
    const preset = parse({ ...value, id: crypto.randomUUID(), name });
    if (!preset) {
      addToast(
        "Check the filters and use a preset name of at most 60 characters.",
        "error",
      );
      return;
    }
    if (write([preset, ...read()].slice(0, 20)))
      addToast("Filter preset saved", "success");
  }
  return {
    presets,
    save,
    remove: (id: string) => write(read().filter((preset) => preset.id !== id)),
  };
}
