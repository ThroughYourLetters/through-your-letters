import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import { useToastStore } from "../../store/useToastStore";
import ModerationCard from "./ModerationCard";
import AdminPagination from "./AdminPagination";
import AdminFilterPresets from "./AdminFilterPresets";
import {
  useAdminAction,
  useAdminResource,
  type AdminSessionProps,
} from "./useAdminOperations";
import { useAdminPresets, type QueueStatus } from "./useAdminPresets";

type Action = "approve" | "reject" | "delete" | "keep";
export default function AdminModerationPanel({
  token,
  onSessionExpired,
  mode,
  onModeChange,
  onChanged,
}: AdminSessionProps & {
  mode: "queue" | "reports";
  onModeChange: (mode: "queue" | "reports") => void;
  onChanged: () => void;
}) {
  const addToast = useToastStore((state) => state.addToast);
  const [filters, setFilters] = useState<{
    status: QueueStatus;
    limit: number;
  }>({ status: "PENDING", limit: 25 });
  const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState(new Set<string>());
  const [failures, setFailures] = useState<{ id: string; error: string }[]>([]);
  const presets = useAdminPresets();
  const session = { token, onSessionExpired };
  const action = useAdminAction(session);
  const load = useCallback(
    (signal: AbortSignal) =>
      api.adminGetQueue(
        {
          status: mode === "reports" ? "REPORTED" : filters.status,
          limit: filters.limit,
          offset,
        },
        signal,
      ),
    [mode, filters, offset],
  );
  const resource = useAdminResource(session, load);
  const items = resource.data?.items ?? [];
  const disabled = action.busy || resource.loading;
  useEffect(() => {
    setOffset(0);
    setSelected(new Set());
    setFailures([]);
  }, [mode]);
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
          : Math.floor((resource.data.total - 1) / filters.limit) *
              filters.limit,
      );
  }, [resource.data, offset, filters.limit]);
  function changeFilters(next: typeof filters) {
    setFilters(next);
    setOffset(0);
    setSelected(new Set());
    setFailures([]);
  }
  function changePage(next: number) {
    setOffset(next);
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
  function reasonFor(kind: Action): string | undefined | null {
    if (kind === "reject") {
      const value = window.prompt("Reason for rejection:");
      return value === null ? null : value.trim() || "Administrative rejection";
    }
    if (
      kind === "delete" &&
      !window.confirm("Permanently delete the selected uploads?")
    )
      return null;
    return undefined;
  }
  async function single(id: string, kind: Action) {
    const reason = reasonFor(kind);
    if (reason === null) return;
    const result = await action.run(() =>
      kind === "approve"
        ? api.adminApprove(id)
        : kind === "keep"
          ? api.adminClearReports(id)
          : kind === "reject"
            ? api.adminReject(id, reason || "Administrative rejection")
            : api.adminDelete(id),
    );
    if (!result.ok) return;
    addToast(
      {
        approve: "Upload approved",
        keep: "Reports cleared",
        reject: "Upload rejected",
        delete: "Upload deleted",
      }[kind],
      "success",
    );
    setFailures([]);
    resource.reload();
    onChanged();
  }
  async function bulk(kind: Action) {
    if (!selected.size) return;
    const reason = reasonFor(kind);
    if (reason === null) return;
    const result = await action.run(() =>
      api.adminBulkLetterings({ ids: [...selected], action: kind, reason }),
    );
    if (!result.ok) return;
    setFailures(result.value.failed_items);
    setSelected(new Set(result.value.failed_items.map((item) => item.id)));
    addToast(
      `${result.value.processed} upload${result.value.processed === 1 ? "" : "s"} updated; ${result.value.failed} failed`,
      result.value.failed ? "warning" : "success",
    );
    resource.reload();
    onChanged();
  }
  const choices: QueueStatus[] = ["PENDING", "APPROVED", "REJECTED"];
  return (
    <section className="space-y-6" aria-labelledby="moderation-title">
      <h2 id="moderation-title" className="font-display text-2xl">
        {mode === "reports" ? "Flagged uploads" : "Upload moderation"}
      </h2>
      {mode === "reports" && (
        <p>Review reported uploads, then clear their flags or delete them.</p>
      )}
      <div className="card p-4 space-y-4">
        <div className="flex flex-wrap gap-3 items-center">
          {mode === "queue" &&
            choices.map((status) => (
              <button
                key={status}
                aria-pressed={filters.status === status}
                className="btn-outline"
                disabled={action.busy}
                onClick={() => changeFilters({ ...filters, status })}
              >
                {status}
              </button>
            ))}
          <label>
            Uploads per page{" "}
            <select
              className="border border-line p-2"
              aria-label="Uploads per page"
              value={filters.limit}
              disabled={action.busy}
              onChange={(event) =>
                changeFilters({ ...filters, limit: Number(event.target.value) })
              }
            >
              {[10, 25, 50, 100].map((size) => (
                <option key={size} value={size}>
                  {size}
                </option>
              ))}
            </select>
          </label>
          <button
            className="btn-outline"
            disabled={disabled}
            onClick={resource.reload}
          >
            Refresh uploads
          </button>
        </div>
        <AdminFilterPresets
          items={presets.presets.filter((preset) => preset.type === "queue")}
          onSave={() =>
            presets.save({
              type: "queue",
              payload: {
                statusFilter: filters.status,
                queueLimit: filters.limit,
                tab: mode,
              },
            })
          }
          onRemove={presets.remove}
          onApply={(id) => {
            const preset = presets.presets.find((item) => item.id === id);
            if (preset?.type !== "queue") return;
            changeFilters({
              status: preset.payload.statusFilter,
              limit: preset.payload.queueLimit,
            });
            onModeChange(preset.payload.tab);
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
            onClick={() => void bulk(mode === "reports" ? "keep" : "approve")}
          >
            {mode === "reports"
              ? "Clear selected flags"
              : "Approve selected uploads"}
          </button>
          {mode === "queue" && (
            <button
              className="btn-outline"
              disabled={disabled || !selected.size}
              onClick={() => void bulk("reject")}
            >
              Reject selected uploads
            </button>
          )}
          <button
            className="btn-outline"
            disabled={disabled || !selected.size}
            onClick={() => void bulk("delete")}
          >
            Delete selected uploads
          </button>
        </div>
        {resource.data && (
          <AdminPagination
            offset={offset}
            limit={filters.limit}
            total={resource.data.total}
            disabled={disabled}
            onChange={changePage}
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
          <p>These uploads were not changed:</p>
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
        <p role="status">Loading uploads…</p>
      ) : resource.data && items.length === 0 ? (
        <p>No uploads match these filters.</p>
      ) : (
        <div className="grid gap-5">
          {items.map((item) => (
            <ModerationCard
              key={item.id}
              item={item}
              selected={selected.has(item.id)}
              onToggleSelected={() => toggle(item.id)}
              isProcessing={action.busy}
              isReported={mode === "reports"}
              onApprove={() =>
                void single(item.id, mode === "reports" ? "keep" : "approve")
              }
              onReject={
                mode === "queue"
                  ? () => void single(item.id, "reject")
                  : undefined
              }
              onDelete={() => void single(item.id, "delete")}
            />
          ))}
        </div>
      )}
    </section>
  );
}
