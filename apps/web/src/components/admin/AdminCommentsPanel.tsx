import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import { useToastStore } from "../../store/useToastStore";
import CommentModerationCard from "./CommentModerationCard";
import AdminPagination from "./AdminPagination";
import AdminFilterPresets from "./AdminFilterPresets";
import {
  useAdminAction,
  useAdminResource,
  type AdminSessionProps,
} from "./useAdminOperations";
import { useAdminPresets, type CommentFilters } from "./useAdminPresets";

type Action = "hide" | "restore" | "delete";
const initial: CommentFilters = {
  commentStatusFilter: "ALL",
  commentQuery: "",
  reviewOnly: false,
  minScore: 0,
  commentSort: "priority",
  commentLimit: 25,
};
export default function AdminCommentsPanel({
  token,
  onSessionExpired,
  onChanged,
}: AdminSessionProps & { onChanged: () => void }) {
  const addToast = useToastStore((state) => state.addToast);
  const [filters, setFilters] = useState(initial);
  const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState(new Set<string>());
  const [failures, setFailures] = useState<{ id: string; error: string }[]>([]);
  const presets = useAdminPresets();
  const session = { token, onSessionExpired };
  const action = useAdminAction(session);
  const load = useCallback(
    (signal: AbortSignal) =>
      api.adminGetComments(
        {
          status: filters.commentStatusFilter,
          q: filters.commentQuery.trim() || undefined,
          needs_review: filters.reviewOnly || undefined,
          min_score: filters.minScore || undefined,
          sort: filters.commentSort,
          limit: filters.commentLimit,
          offset,
        },
        signal,
      ),
    [filters, offset],
  );
  const resource = useAdminResource(session, load);
  const items = resource.data?.items ?? [];
  const disabled = action.busy || resource.loading;
  useEffect(() => {
    if (!resource.data) return;
    const ids = new Set(resource.data.items.map((item) => item.id));
    setSelected(
      (previous) => new Set([...previous].filter((id) => ids.has(id))),
    );
    if (offset >= resource.data.total && offset > 0)
      setOffset(
        resource.data.total === 0
          ? 0
          : Math.floor((resource.data.total - 1) / filters.commentLimit) *
              filters.commentLimit,
      );
  }, [resource.data, offset, filters.commentLimit]);
  function changeFilters(next: CommentFilters) {
    setFilters(next);
    setOffset(0);
    setSelected(new Set());
    setFailures([]);
  }
  function toggle(id: string) {
    setSelected((previous) => {
      const next = new Set(previous);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }
  async function perform(kind: Action, id?: string) {
    const ids = id ? [id] : [...selected];
    if (!ids.length) return;
    let reason: string | undefined;
    if (kind === "hide") {
      const input = window.prompt("Reason for hiding the selected comments?");
      if (input === null) return;
      reason = input.trim() || "Hidden by moderation";
    }
    if (
      kind === "delete" &&
      !window.confirm("Permanently delete the selected comments?")
    )
      return;
    const result = await action.run(async () => {
      if (!id) return api.adminBulkComments({ ids, action: kind, reason });
      if (kind === "hide") await api.adminHideComment(id, reason);
      else if (kind === "restore") await api.adminRestoreComment(id);
      else await api.adminDeleteComment(id);
      return { processed: 1, failed: 0, failed_items: [] };
    });
    if (!result.ok) return;
    setFailures(result.value.failed_items);
    setSelected(new Set(result.value.failed_items.map((item) => item.id)));
    addToast(
      `${result.value.processed} comment${result.value.processed === 1 ? "" : "s"} updated; ${result.value.failed} failed`,
      result.value.failed ? "warning" : "success",
    );
    resource.reload();
    onChanged();
  }
  return (
    <section className="space-y-6" aria-labelledby="comments-title">
      <h2 id="comments-title" className="font-display text-2xl">
        Comment moderation
      </h2>
      <div className="card p-4 space-y-4">
        <div className="flex flex-wrap gap-3 items-center">
          {(["ALL", "VISIBLE", "HIDDEN"] as const).map((status) => (
            <button
              key={status}
              className="btn-outline"
              aria-pressed={filters.commentStatusFilter === status}
              disabled={action.busy}
              onClick={() =>
                changeFilters({ ...filters, commentStatusFilter: status })
              }
            >
              {status}
            </button>
          ))}
          <label>
            Comments per page{" "}
            <select
              className="border border-line p-2"
              value={filters.commentLimit}
              disabled={action.busy}
              onChange={(event) =>
                changeFilters({
                  ...filters,
                  commentLimit: Number(event.target.value),
                })
              }
            >
              {[10, 25, 50, 100].map((size) => (
                <option key={size} value={size}>
                  {size}
                </option>
              ))}
            </select>
          </label>
          <label>
            Search comments{" "}
            <input
              className="border border-line p-2 max-w-full"
              maxLength={500}
              value={filters.commentQuery}
              disabled={action.busy}
              onChange={(event) =>
                changeFilters({ ...filters, commentQuery: event.target.value })
              }
            />
          </label>
          <button
            className="btn-outline"
            disabled={disabled}
            onClick={resource.reload}
          >
            Refresh comments
          </button>
        </div>
        <div className="flex flex-wrap gap-4 items-center">
          <label>
            <input
              type="checkbox"
              checked={filters.reviewOnly}
              disabled={action.busy}
              onChange={(event) =>
                changeFilters({ ...filters, reviewOnly: event.target.checked })
              }
            />{" "}
            Needs review only
          </label>
          <label>
            Minimum score{" "}
            <input
              className="border border-line p-2 w-20"
              type="number"
              min={0}
              max={100}
              value={filters.minScore}
              disabled={action.busy}
              onChange={(event) =>
                changeFilters({
                  ...filters,
                  minScore: Math.min(
                    100,
                    Math.max(0, Math.trunc(Number(event.target.value)) || 0),
                  ),
                })
              }
            />
          </label>
          <label>
            Sort comments{" "}
            <select
              className="border border-line p-2"
              value={filters.commentSort}
              disabled={action.busy}
              onChange={(event) => {
                const sort = event.target.value;
                if (
                  sort === "priority" ||
                  sort === "newest" ||
                  sort === "score"
                )
                  changeFilters({ ...filters, commentSort: sort });
              }}
            >
              <option value="priority">Priority</option>
              <option value="score">Score</option>
              <option value="newest">Newest</option>
            </select>
          </label>
        </div>
        <AdminFilterPresets
          items={presets.presets.filter((preset) => preset.type === "comments")}
          onSave={() => presets.save({ type: "comments", payload: filters })}
          onRemove={presets.remove}
          onApply={(id) => {
            const preset = presets.presets.find((item) => item.id === id);
            if (preset?.type === "comments") changeFilters(preset.payload);
          }}
        />
        <div className="flex flex-wrap gap-3 items-center">
          <label>
            <input
              type="checkbox"
              checked={items.length > 0 && selected.size === items.length}
              disabled={disabled}
              onChange={(event) =>
                setSelected(
                  event.target.checked
                    ? new Set(items.map((item) => item.id))
                    : new Set(),
                )
              }
            />{" "}
            Select page
          </label>
          <button
            className="btn-outline"
            disabled={disabled || !selected.size}
            onClick={() => void perform("hide")}
          >
            Hide selected comments
          </button>
          <button
            className="btn-outline"
            disabled={disabled || !selected.size}
            onClick={() => void perform("restore")}
          >
            Restore selected comments
          </button>
          <button
            className="btn-outline"
            disabled={disabled || !selected.size}
            onClick={() => void perform("delete")}
          >
            Delete selected comments
          </button>
        </div>
        {resource.data && (
          <AdminPagination
            offset={offset}
            limit={filters.commentLimit}
            total={resource.data.total}
            disabled={disabled}
            onChange={(next) => {
              setOffset(next);
              setSelected(new Set());
              setFailures([]);
            }}
          />
        )}
      </div>
      {resource.error && (
        <p role="alert">
          {resource.error}{" "}
          <button className="underline" onClick={resource.reload}>
            Try again
          </button>
        </p>
      )}
      {failures.length > 0 && (
        <div role="alert">
          <p>These comments were not changed:</p>
          <ul>
            {failures.map((item) => (
              <li key={item.id}>
                {item.id}: {item.error}
              </li>
            ))}
          </ul>
        </div>
      )}
      {resource.loading ? (
        <p role="status">Loading comments…</p>
      ) : resource.data && items.length === 0 ? (
        <p>No comments match these filters.</p>
      ) : (
        <div className="grid gap-4">
          {items.map((item) => (
            <CommentModerationCard
              key={item.id}
              item={item}
              isProcessing={action.busy}
              selected={selected.has(item.id)}
              onToggleSelected={() => toggle(item.id)}
              onHide={() => void perform("hide", item.id)}
              onRestore={() => void perform("restore", item.id)}
              onDelete={() => void perform("delete", item.id)}
            />
          ))}
        </div>
      )}
    </section>
  );
}
