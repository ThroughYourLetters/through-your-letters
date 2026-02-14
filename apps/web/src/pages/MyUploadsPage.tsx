import React, { useCallback, useEffect, useMemo, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useNavigate } from "react-router-dom";
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
  const { addToast } = useToastStore();
  const { user, hydrated, hydrate } = useAuthStore();

  const [status, setStatus] = useState("ALL");
  const [items, setItems] = useState<MyUploadItem[]>([]);
  const [loading, setLoading] = useState(true);
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

  useEffect(() => {
    if (!hydrated) hydrate();
  }, [hydrated, hydrate]);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const data = await api.getMyUploads({
        status: status === "ALL" ? undefined : status,
        limit: 50,
        offset: 0,
      });
      setItems(data.items);
    } catch (err) {
      addToast(
        err instanceof Error ? err.message : "Failed to load uploads",
        "error",
      );
      if ((err as Error)?.message?.includes("401")) {
        navigate("/auth");
      }
    } finally {
      setLoading(false);
    }
  }, [status, addToast, navigate]);

  useEffect(() => {
    if (user) load();
  }, [user, load]);

  const statusCounts = useMemo(() => {
    const m = new Map<string, number>();
    for (const s of statuses) m.set(s, 0);
    for (const item of items) {
      m.set("ALL", (m.get("ALL") || 0) + 1);
      m.set(item.status, (m.get(item.status) || 0) + 1);
    }
    return m;
  }, [items]);

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
    if (!/^\d{6}$/.test(draftPinCode.trim())) {
      nextErrors.pin_code = "PIN code must be exactly 6 digits.";
    }

    setEditErrors(nextErrors);
    return Object.keys(nextErrors).length === 0;
  };

  const saveEdit = async (id: string) => {
    if (!validateEditDraft()) return;

    try {
      const updated = await api.updateMyUpload(id, {
        description: draftDesc.trim(),
        contributor_tag: draftContributorTag.trim(),
        pin_code: draftPinCode.trim(),
      });
      setItems((prev) => prev.map((p) => (p.id === id ? updated : p)));
      setEditingId(null);
      setTimelineById((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      addToast("Upload updated", "success");
    } catch (err) {
      addToast(err instanceof Error ? err.message : "Update failed", "error");
    }
  };

  const toggleTimeline = async (id: string) => {
    if (expandedTimelineId === id) {
      setExpandedTimelineId(null);
      return;
    }

    setExpandedTimelineId(id);
    if (timelineById[id]) return;

    setTimelineLoadingId(id);
    try {
      const timeline = await api.getMyUploadTimeline(id);
      setTimelineById((prev) => ({ ...prev, [id]: timeline }));
    } catch (err) {
      addToast(
        err instanceof Error ? err.message : "Timeline load failed",
        "error",
      );
    } finally {
      setTimelineLoadingId(null);
    }
  };

  const deleteMine = async (id: string) => {
    if (!window.confirm("Delete this upload?")) return;
    try {
      await api.deleteOwnLettering(id);
      setItems((prev) => prev.filter((p) => p.id !== id));
      addToast("Upload deleted", "success");
    } catch (err) {
      addToast(err instanceof Error ? err.message : "Delete failed", "error");
    }
  };

  if (!hydrated) {
    return (
      <div className="flex justify-center py-20">
        <Loader2 size={32} className="animate-spin text-[#cc543a]" />
      </div>
    );
  }

  if (!user) {
    return (
      <div className="max-w-xl mx-auto py-20 text-center space-y-6">
        <h1 className="text-4xl font-black uppercase tracking-tighter">
          Sign In Required
        </h1>
        <p className="text-slate-500">
          You need an account to manage your uploads.
        </p>
        <Link
          to="/auth"
          className="bg-black text-white px-6 py-3 text-[10px] font-black uppercase hover:bg-[#cc543a] transition-colors"
        >
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
        <div className="border-b-4 border-black pb-8">
          <h1 className="text-4xl md:text-6xl font-black uppercase tracking-tighter">
            My Uploads
          </h1>
          <p className="text-[10px] font-black uppercase text-slate-400 tracking-widest mt-2">
            Manage your contributions and update descriptions.
          </p>
        </div>

        <div className="flex flex-wrap gap-2 border-2 border-black bg-white p-3">
          {statuses.map((s) => (
            <button
              key={s}
              onClick={() => setStatus(s)}
              className={`px-3 py-2 text-[10px] font-black uppercase border-2 border-black ${status === s ? "bg-black text-white" : "bg-white"}`}
            >
              {s} ({statusCounts.get(s) || 0})
            </button>
          ))}
        </div>

        {loading ? (
          <div className="flex justify-center py-20">
            <Loader2 size={32} className="animate-spin text-[#cc543a]" />
          </div>
        ) : items.length === 0 ? (
          <div className="text-center py-20 border-4 border-dashed border-black/20 text-slate-400 font-black uppercase">
            No uploads found
          </div>
        ) : (
          <div className="grid gap-6">
            {items.map((item) => (
              <div
                key={item.id}
                className="border-4 border-black bg-white p-6 grid grid-cols-1 md:grid-cols-[180px_1fr] gap-6"
              >
                <button
                  onClick={() => navigate(`/lettering/${item.id}`)}
                  className="border-2 border-black overflow-hidden"
                >
                  <img
                    src={item.thumbnail_small || item.image_url}
                    alt={item.detected_text || "upload"}
                    className="w-full h-44 object-cover"
                  />
                </button>

                <div className="space-y-4">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-[9px] font-black uppercase bg-black text-white px-2 py-1">
                      {item.status}
                    </span>
                    <span className="text-[9px] font-black uppercase text-slate-500">
                      PIN {item.pin_code}
                    </span>
                    <span className="text-[9px] font-black uppercase text-slate-500">
                      {new Date(item.created_at).toLocaleDateString()}
                    </span>
                  </div>

                  <h3 className="text-2xl font-black uppercase tracking-tighter">
                    {item.detected_text || "Street Discovery"}
                  </h3>

                  <div className="border-l-4 border-[#cc543a] bg-slate-50 p-3 space-y-1">
                    <p className="text-[10px] font-black uppercase tracking-widest text-[#cc543a] flex items-center gap-1">
                      <AlertTriangle size={12} />
                      Moderation Feedback
                    </p>
                    <p className="text-sm text-slate-700">
                      {item.moderation_reason ||
                        "No moderation note recorded yet."}
                    </p>
                    <p className="text-[10px] font-bold uppercase text-slate-500 flex items-center gap-1">
                      <Clock3 size={12} />
                      {item.moderated_at
                        ? `Action ${new Date(item.moderated_at).toLocaleString()}`
                        : "Action timestamp unavailable"}
                    </p>
                    <p className="text-[10px] font-bold uppercase text-slate-500">
                      Next Step: {getNextSteps(item.status)}
                    </p>
                  </div>

                  {editingId === item.id ? (
                    <div className="space-y-3">
                      <input
                        value={draftContributorTag}
                        onChange={(e) => setDraftContributorTag(e.target.value)}
                        className="w-full border-2 border-black p-3 text-sm outline-none focus:border-[#cc543a]"
                        placeholder="Contributor tag"
                      />
                      {editErrors.contributor_tag && (
                        <p className="text-xs font-bold text-red-700">
                          {editErrors.contributor_tag}
                        </p>
                      )}
                      <input
                        value={draftPinCode}
                        onChange={(e) =>
                          setDraftPinCode(
                            e.target.value.replace(/\D/g, "").slice(0, 6),
                          )
                        }
                        className="w-full border-2 border-black p-3 text-sm outline-none focus:border-[#cc543a]"
                        placeholder="PIN code"
                      />
                      {editErrors.pin_code && (
                        <p className="text-xs font-bold text-red-700">
                          {editErrors.pin_code}
                        </p>
                      )}
                      <textarea
                        value={draftDesc}
                        onChange={(e) => setDraftDesc(e.target.value)}
                        rows={3}
                        className="w-full border-2 border-black p-3 text-sm outline-none focus:border-[#cc543a]"
                      />
                      {editErrors.description && (
                        <p className="text-xs font-bold text-red-700">
                          {editErrors.description}
                        </p>
                      )}
                      <div className="flex gap-2">
                        <button
                          onClick={() => saveEdit(item.id)}
                          className="inline-flex items-center gap-2 bg-black text-white px-4 py-2 text-[10px] font-black uppercase hover:bg-[#cc543a] transition-colors"
                        >
                          <Save size={14} /> Save
                        </button>
                        <button
                          onClick={() => setEditingId(null)}
                          className="border-2 border-black px-4 py-2 text-[10px] font-black uppercase"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  ) : (
                    <p className="text-sm text-slate-700">
                      {item.description || "No description."}
                    </p>
                  )}

                  <div className="flex gap-2">
                    {editingId !== item.id && (
                      <button
                        onClick={() => startEdit(item)}
                        className="inline-flex items-center gap-2 border-2 border-black px-4 py-2 text-[10px] font-black uppercase hover:bg-black hover:text-white transition-colors"
                      >
                        <Edit3 size={14} /> Edit
                      </button>
                    )}
                    <button
                      onClick={() => void toggleTimeline(item.id)}
                      className="inline-flex items-center gap-2 border-2 border-black px-4 py-2 text-[10px] font-black uppercase hover:bg-slate-100 transition-colors"
                    >
                      <ListChecks size={14} />
                      {expandedTimelineId === item.id
                        ? "Hide Timeline"
                        : "Timeline"}
                    </button>
                    <button
                      onClick={() => deleteMine(item.id)}
                      className="inline-flex items-center gap-2 border-2 border-black px-4 py-2 text-[10px] font-black uppercase text-red-600 hover:bg-red-600 hover:text-white transition-colors"
                    >
                      <Trash2 size={14} /> Delete
                    </button>
                  </div>

                  {expandedTimelineId === item.id && (
                    <div className="border-2 border-black bg-slate-50 p-4 space-y-4">
                      <div>
                        <p className="text-[10px] font-black uppercase tracking-widest text-slate-500">
                          Status Timeline
                        </p>
                        {timelineLoadingId === item.id ? (
                          <div className="flex py-4">
                            <Loader2
                              size={18}
                              className="animate-spin text-[#cc543a]"
                            />
                          </div>
                        ) : (timelineById[item.id]?.status_history || [])
                            .length === 0 ? (
                          <p className="text-sm text-slate-500 py-2">
                            No timeline entries available yet.
                          </p>
                        ) : (
                          <div className="space-y-2 pt-2">
                            {timelineById[item.id].status_history.map(
                              (entry) => (
                                <div
                                  key={entry.id}
                                  className="border border-black bg-white px-3 py-2 text-xs"
                                >
                                  <p className="font-black uppercase">
                                    {entry.from_status
                                      ? `${entry.from_status} -> `
                                      : ""}
                                    {entry.to_status}
                                  </p>
                                  <p className="text-slate-600">
                                    {entry.reason || "No reason recorded"}
                                  </p>
                                  <p className="text-slate-400 uppercase font-bold">
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
                      </div>

                      <div>
                        <p className="text-[10px] font-black uppercase tracking-widest text-slate-500">
                          Metadata Change History
                        </p>
                        {timelineLoadingId === item.id ? null : (
                          <div className="space-y-2 pt-2">
                            {(timelineById[item.id]?.metadata_history || [])
                              .length === 0 ? (
                              <p className="text-sm text-slate-500">
                                No metadata edits recorded.
                              </p>
                            ) : (
                              timelineById[item.id].metadata_history.map(
                                (entry) => (
                                  <div
                                    key={entry.id}
                                    className="border border-black bg-white px-3 py-2 text-xs"
                                  >
                                    <p className="font-black uppercase">
                                      {entry.field_name}
                                    </p>
                                    <p className="text-slate-600 break-all">
                                      {entry.old_value || "(empty)"}
                                      {" -> "}
                                      {entry.new_value || "(empty)"}
                                    </p>
                                    <p className="text-slate-400 uppercase font-bold">
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
                      </div>
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </>
  );
};

export default MyUploadsPage;
