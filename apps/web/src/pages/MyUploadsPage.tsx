import React, { useEffect, useRef, useState } from "react";
import { postalCodeSchema } from "@ttl/validation";
import { Helmet } from "react-helmet-async";
import { Link, useNavigate } from "react-router-dom";
import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";
import {
  Edit3,
  Save,
  Loader2,
  Trash2,
  Clock3,
  ListChecks,
  AlertTriangle,
} from "lucide-react";
import { api, MyUploadItem, MyUploadTimelineResponse } from "../lib/api";
import { useAuthStore } from "../store/useAuthStore";
import { useToastStore } from "../store/useToastStore";
import { invalidatePublicLetterings } from "../lib/publicQueries";
import { nextAccountPage } from "../lib/pagination";
import { captureUserSession, isCurrentUserSession } from "../lib/userSession";

const statuses = ["ALL", "PENDING", "APPROVED", "REJECTED", "REPORTED"];

const contributorTagRegex = /^[A-Za-z0-9 _.-]+$/;

const getNextSteps = (status: string) => {
  switch (status) {
    case "PENDING":
      return "No action needed. Moderation review is in progress.";
    case "REJECTED":
      return "Update metadata, then upload a clearer capture if needed.";
    case "REPORTED":
      return "Await moderator review. You can update context metadata now.";
    default:
      return "Visible in discovery. Keep metadata accurate for researchers.";
  }
};

const MyUploadsPage: React.FC = () => {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { addToast } = useToastStore();
  const { user, hydrated, hydrate, revision } = useAuthStore();

  const [status, setStatus] = useState("ALL");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draftDesc, setDraftDesc] = useState("");
  const [draftContributorTag, setDraftContributorTag] = useState("");
  const [draftPinCode, setDraftPinCode] = useState("");
  const [editErrors, setEditErrors] = useState<{
    description?: string;
    contributor_tag?: string;
    pin_code?: string;
  }>({});
  const [expandedTimelineId, setExpandedTimelineId] = useState<string | null>(
    null,
  );
  const [timelineLoadingId, setTimelineLoadingId] = useState<string | null>(
    null,
  );
  const [timelineById, setTimelineById] = useState<
    Record<string, MyUploadTimelineResponse>
  >({});
  const [timelineErrors, setTimelineErrors] = useState<Record<string, string>>(
    {},
  );
  const [historyBusy, setHistoryBusy] = useState<string | null>(null);
  const historyPending = useRef(false);
  const timelinePending = useRef(new Set<string>());
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const uploads = useInfiniteQuery({
    queryKey: ["my-uploads", user?.id, revision, status],
    enabled: !!user,
    initialPageParam: 0 as number | string,
    queryFn: ({ pageParam, signal }) =>
      api.getMyUploads({
        status: status === "ALL" ? undefined : status,
        limit: 50,
        ...(typeof pageParam === "string"
          ? { after: pageParam }
          : { offset: pageParam }),
        signal,
      }),
    getNextPageParam: nextAccountPage,
  });
  const items = [
    ...new Map(
      (uploads.data?.pages.flatMap((page) => page.items) ?? []).map((item) => [
        item.id,
        item,
      ]),
    ).values(),
  ];
  const total = uploads.data?.pages[0]?.total;
  const loading = uploads.isPending;

  useEffect(() => {
    if (!hydrated) hydrate();
  }, [hydrated, hydrate]);

  const startEdit = (item: MyUploadItem) => {
    setEditingId(item.id);
    setDraftDesc(item.description || "");
    setDraftContributorTag(item.contributor_tag || "");
    setDraftPinCode(item.pin_code || "");
    setEditErrors({});
  };

  const validateEditDraft = () => {
    const nextErrors: {
      description?: string;
      contributor_tag?: string;
      pin_code?: string;
    } = {};

    if (draftDesc.trim().length > 1200) {
      nextErrors.description = "Description must be 1200 characters or less.";
    }
    const contributor = draftContributorTag.trim();
    if (contributor.length < 2 || contributor.length > 30) {
      nextErrors.contributor_tag =
        "Contributor tag must be 2 to 30 characters.";
    } else if (!contributorTagRegex.test(contributor)) {
      nextErrors.contributor_tag =
        "Contributor tag can include letters, numbers, spaces, _, -, .";
    }
    const postal = postalCodeSchema.safeParse(draftPinCode);
    if (!postal.success) {
      nextErrors.pin_code = postal.error.issues[0].message;
    }

    setEditErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0 || !postal.success) return null;
    return {
      description: draftDesc.trim(),
      contributor_tag: contributor,
      pin_code: postal.data,
    };
  };

  const saveEdit = async (id: string) => {
    const draft = validateEditDraft();
    if (!draft) return;

    try {
      const postalChanged =
        items.find((item) => item.id === id)?.pin_code !== draft.pin_code;
      await api.updateMyUpload(id, draft);
      await uploads.refetch();
      setEditingId(null);
      setTimelineById((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      await invalidatePublicLetterings(queryClient);
      addToast(
        postalChanged
          ? "Upload updated. Its location needs review."
          : "Upload updated",
        "success",
      );
    } catch (err) {
      addToast(err instanceof Error ? err.message : "Update failed", "error");
    }
  };

  const toggleTimeline = async (id: string, retry = false) => {
    if (expandedTimelineId === id && !retry) {
      setExpandedTimelineId(null);
      return;
    }

    setExpandedTimelineId(id);
    if (timelineById[id]) return;

    setTimelineLoadingId(id);
    if (timelinePending.current.has(id)) return;
    timelinePending.current.add(id);
    setTimelineErrors((previous) => ({ ...previous, [id]: "" }));
    const session = captureUserSession();
    const current = () => mounted.current && isCurrentUserSession(session);
    try {
      const timeline = await api.getMyUploadTimeline(id);
      if (current()) setTimelineById((prev) => ({ ...prev, [id]: timeline }));
    } catch (err) {
      if (current())
        setTimelineErrors((previous) => ({
          ...previous,
          [id]:
            err instanceof Error ? err.message : "Timeline could not be loaded",
        }));
    } finally {
      timelinePending.current.delete(id);
      if (current())
        setTimelineLoadingId((loadingId) =>
          loadingId === id ? null : loadingId,
        );
    }
  };

  const loadOlderHistory = async (
    id: string,
    section: "status" | "metadata",
  ) => {
    const before = timelineById[id];
    const after =
      section === "status"
        ? before?.next_status_cursor
        : before?.next_metadata_cursor;
    if (!after || historyPending.current) return;
    historyPending.current = true;
    setHistoryBusy(`${id}:${section}`);
    const session = captureUserSession();
    const current = () => mounted.current && isCurrentUserSession(session);
    try {
      const page = await api.getMyUploadTimeline(id, { section, after });
      if (current())
        setTimelineById((previous) => {
          const existing = previous[id];
          if (!existing) return previous;
          return {
            ...previous,
            [id]:
              section === "status"
                ? {
                    ...existing,
                    status_history: [
                      ...new Map(
                        [
                          ...existing.status_history,
                          ...page.status_history,
                        ].map((item) => [item.id, item]),
                      ).values(),
                    ],
                    next_status_cursor: page.next_status_cursor,
                  }
                : {
                    ...existing,
                    metadata_history: [
                      ...new Map(
                        [
                          ...existing.metadata_history,
                          ...page.metadata_history,
                        ].map((item) => [item.id, item]),
                      ).values(),
                    ],
                    next_metadata_cursor: page.next_metadata_cursor,
                  },
          };
        });
    } catch {
      if (current())
        addToast("Older history could not be loaded. Please retry.", "error");
    } finally {
      historyPending.current = false;
      if (current()) setHistoryBusy(null);
    }
  };

  const deleteMine = async (id: string) => {
    if (!window.confirm("Delete this upload?")) return;
    try {
      await api.deleteOwnLettering(id);
      await uploads.refetch();
      addToast("Upload deleted", "success");
    } catch (err) {
      addToast(err instanceof Error ? err.message : "Delete failed", "error");
    }
  };

  if (!hydrated) {
    return (
      <div className="flex justify-center py-20">
        <Loader2 size={32} className="animate-spin text-terracotta" />
      </div>
    );
  }

  if (!user) {
    return (
      <div className="max-w-xl mx-auto py-20 text-center space-y-6">
        <h1 className="font-display font-semibold text-4xl tracking-tight">
          Sign In Required
        </h1>
        <p className="text-stone-500">
          You need an account to manage your uploads.
        </p>
        <Link to="/auth" className="btn-dark">
          Go to Auth
        </Link>
      </div>
    );
  }

  return (
    <>
      <Helmet>
        <title>My Uploads | Through Your Letters</title>
      </Helmet>
      <div className="space-y-10 pb-24">
        <div className="border-b border-line pb-8">
          <h1 className="font-display font-semibold text-4xl md:text-6xl tracking-tight">
            My Uploads
          </h1>
          <p className="text-xs font-semibold uppercase text-stone-400 tracking-widest mt-2">
            Manage your contributions and update descriptions.
          </p>
          <Link className="btn-outline mt-4" to="/me/queue">
            Saved uploads on this device
          </Link>
        </div>

        <div className="flex flex-wrap gap-1 w-fit rounded-3xl border border-line bg-surface p-1">
          {statuses.map((s) => (
            <button
              key={s}
              onClick={() => setStatus(s)}
              className={`rounded-full px-4 py-2 text-sm font-medium transition-colors whitespace-nowrap ${status === s ? "bg-ink text-paper" : "text-muted hover:text-ink"}`}
            >
              {s}
            </button>
          ))}
        </div>

        {total !== undefined && (
          <p className="text-sm text-muted">
            Showing {items.length} of {total} uploads
            {status === "ALL" ? "" : ` with status ${status}`}.
          </p>
        )}

        {uploads.error && (
          <p role="alert">
            Uploads could not be loaded.{" "}
            <button
              className="underline"
              onClick={() =>
                void (uploads.isFetchNextPageError
                  ? uploads.fetchNextPage()
                  : uploads.refetch())
              }
            >
              Try again
            </button>
          </p>
        )}

        {loading ? (
          <div className="flex justify-center py-20">
            <Loader2 size={32} className="animate-spin text-terracotta" />
          </div>
        ) : items.length === 0 && !uploads.error ? (
          <div className="text-center py-20 border border-dashed border-line text-stone-400 font-semibold uppercase">
            No uploads found
          </div>
        ) : (
          <div className="grid gap-6">
            {items.map((item) => (
              <div
                key={item.id}
                className="card p-6 grid grid-cols-1 md:grid-cols-[180px_1fr] gap-6"
              >
                <button
                  onClick={() => navigate(`/lettering/${item.id}`)}
                  className="border border-line overflow-hidden"
                >
                  <img
                    src={item.thumbnail_small || item.image_url}
                    alt={item.detected_text || "upload"}
                    className="w-full h-44 object-cover"
                  />
                </button>

                <div className="space-y-4">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-xs font-semibold uppercase bg-ink text-white px-2 py-1">
                      {item.status}
                    </span>
                    <span className="text-xs font-semibold uppercase text-stone-500">
                      PIN {item.pin_code}
                    </span>
                    <span className="text-xs font-semibold uppercase text-stone-500">
                      {new Date(item.created_at).toLocaleDateString()}
                    </span>
                  </div>

                  <h3 className="font-display font-semibold tracking-tight text-2xl">
                    {item.detected_text || "Street Discovery"}
                  </h3>

                  <div className="border-l border-terracotta bg-stone-50 p-3 space-y-1">
                    <p className="text-xs font-semibold uppercase tracking-widest text-terracotta flex items-center gap-1">
                      <AlertTriangle size={12} />
                      Moderation Feedback
                    </p>
                    <p className="text-sm text-stone-700">
                      {item.moderation_reason ||
                        "No moderation note recorded yet."}
                    </p>
                    <p className="text-xs font-bold uppercase text-stone-500 flex items-center gap-1">
                      <Clock3 size={12} />
                      {item.moderated_at
                        ? `Action ${new Date(item.moderated_at).toLocaleString()}`
                        : "Action timestamp unavailable"}
                    </p>
                    <p className="text-xs font-bold uppercase text-stone-500">
                      Next Step: {getNextSteps(item.status)}
                    </p>
                  </div>

                  {editingId === item.id ? (
                    <div className="space-y-3">
                      <input
                        value={draftContributorTag}
                        onChange={(e) => setDraftContributorTag(e.target.value)}
                        className="field w-full"
                        placeholder="Contributor tag"
                      />
                      {editErrors.contributor_tag && (
                        <p className="text-xs font-bold text-red-700">
                          {editErrors.contributor_tag}
                        </p>
                      )}
                      <input
                        value={draftPinCode}
                        aria-label="Postal or ZIP code"
                        aria-describedby="postal-location-policy"
                        onChange={(e) => setDraftPinCode(e.target.value)}
                        className="field w-full"
                        placeholder="Postal / ZIP code"
                      />
                      <p
                        id="postal-location-policy"
                        className="text-sm text-muted"
                      >
                        Changing the postal code removes the map location until
                        it is reviewed.
                      </p>
                      {editErrors.pin_code && (
                        <p className="text-xs font-bold text-red-700">
                          {editErrors.pin_code}
                        </p>
                      )}
                      <textarea
                        value={draftDesc}
                        onChange={(e) => setDraftDesc(e.target.value)}
                        rows={3}
                        className="field w-full"
                      />
                      {editErrors.description && (
                        <p className="text-xs font-bold text-red-700">
                          {editErrors.description}
                        </p>
                      )}
                      <div className="flex gap-2">
                        <button
                          onClick={() => saveEdit(item.id)}
                          className="btn-dark"
                        >
                          <Save size={14} /> Save
                        </button>
                        <button
                          onClick={() => setEditingId(null)}
                          className="border border-line px-4 py-2 text-xs font-semibold uppercase"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  ) : (
                    <p className="text-sm text-stone-700">
                      {item.description || "No description."}
                    </p>
                  )}

                  <div className="flex gap-2">
                    {editingId !== item.id && (
                      <button
                        onClick={() => startEdit(item)}
                        className="inline-flex items-center gap-2 border border-line px-4 py-2 text-xs font-semibold uppercase hover:bg-ink hover:text-white transition-colors"
                      >
                        <Edit3 size={14} /> Edit
                      </button>
                    )}
                    <button
                      onClick={() => void toggleTimeline(item.id)}
                      className="inline-flex items-center gap-2 border border-line px-4 py-2 text-xs font-semibold uppercase hover:bg-stone-100 transition-colors"
                    >
                      <ListChecks size={14} />
                      {expandedTimelineId === item.id
                        ? "Hide Timeline"
                        : "Timeline"}
                    </button>
                    <button
                      onClick={() => deleteMine(item.id)}
                      className="inline-flex items-center gap-2 border border-line px-4 py-2 text-xs font-semibold uppercase text-red-600 hover:bg-red-600 hover:text-white transition-colors"
                    >
                      <Trash2 size={14} /> Delete
                    </button>
                  </div>

                  {expandedTimelineId === item.id && (
                    <div className="border border-line bg-stone-50 p-4 space-y-4">
                      {timelineErrors[item.id] && (
                        <p role="alert">
                          Timeline could not be loaded.{" "}
                          <button
                            className="underline"
                            onClick={() => void toggleTimeline(item.id, true)}
                          >
                            Try again
                          </button>
                        </p>
                      )}
                      <div>
                        <p className="text-xs font-semibold uppercase tracking-widest text-stone-500">
                          Status Timeline
                        </p>
                        {timelineLoadingId === item.id ? (
                          <div className="flex py-4">
                            <Loader2
                              size={18}
                              className="animate-spin text-terracotta"
                            />
                          </div>
                        ) : !timelineErrors[item.id] &&
                          (timelineById[item.id]?.status_history || [])
                            .length === 0 ? (
                          <p className="text-sm text-stone-500 py-2">
                            No timeline entries available yet.
                          </p>
                        ) : (
                          <div className="space-y-2 pt-2">
                            {timelineById[item.id]?.status_history.map(
                              (entry) => (
                                <div
                                  key={entry.id}
                                  className="card px-3 py-2 text-xs"
                                >
                                  <p className="font-semibold uppercase">
                                    {entry.from_status
                                      ? `${entry.from_status} -> `
                                      : ""}
                                    {entry.to_status}
                                  </p>
                                  <p className="text-stone-600">
                                    {entry.reason || "No reason recorded"}
                                  </p>
                                  <p className="text-stone-400 uppercase font-bold">
                                    {new Date(
                                      entry.created_at,
                                    ).toLocaleString()}{" "}
                                    / {entry.actor_type}
                                  </p>
                                </div>
                              ),
                            )}
                          </div>
                        )}
                        {timelineById[item.id]?.next_status_cursor && (
                          <button
                            className="btn-outline mt-3"
                            disabled={historyBusy !== null}
                            onClick={() =>
                              void loadOlderHistory(item.id, "status")
                            }
                          >
                            {historyBusy === `${item.id}:status`
                              ? "Loading older status changes…"
                              : "Load older status changes"}
                          </button>
                        )}
                      </div>

                      <div>
                        <p className="text-xs font-semibold uppercase tracking-widest text-stone-500">
                          Metadata Change History
                        </p>
                        {timelineLoadingId === item.id ||
                        timelineErrors[item.id] ? null : (
                          <div className="space-y-2 pt-2">
                            {(timelineById[item.id]?.metadata_history || [])
                              .length === 0 ? (
                              <p className="text-sm text-stone-500">
                                No metadata edits recorded.
                              </p>
                            ) : (
                              timelineById[item.id].metadata_history.map(
                                (entry) => (
                                  <div
                                    key={entry.id}
                                    className="card px-3 py-2 text-xs"
                                  >
                                    <p className="font-semibold uppercase">
                                      {entry.field_name}
                                    </p>
                                    <p className="text-stone-600 break-all">
                                      {entry.old_value || "(empty)"}
                                      {" -> "}
                                      {entry.new_value || "(empty)"}
                                    </p>
                                    <p className="text-stone-400 uppercase font-bold">
                                      {new Date(
                                        entry.created_at,
                                      ).toLocaleString()}
                                    </p>
                                  </div>
                                ),
                              )
                            )}
                          </div>
                        )}
                        {timelineById[item.id]?.next_metadata_cursor && (
                          <button
                            className="btn-outline mt-3"
                            disabled={historyBusy !== null}
                            onClick={() =>
                              void loadOlderHistory(item.id, "metadata")
                            }
                          >
                            {historyBusy === `${item.id}:metadata`
                              ? "Loading older metadata changes…"
                              : "Load older metadata changes"}
                          </button>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
        {uploads.hasNextPage && (
          <button
            className="btn-outline"
            disabled={uploads.isFetchingNextPage}
            onClick={() => void uploads.fetchNextPage()}
          >
            {uploads.isFetchingNextPage
              ? "Loading more uploads…"
              : "Load more uploads"}
          </button>
        )}
      </div>
    </>
  );
};

export default MyUploadsPage;
