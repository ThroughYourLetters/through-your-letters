import React, { useState, useEffect, useCallback } from "react";
import { Helmet } from "react-helmet-async";
import { useToastStore } from "../store/useToastStore";
import { ADMIN_SESSION_KEY, AdminCommentItem, api } from "../lib/api";
import AdminCitiesPanel from "./admin/AdminCitiesPanel";
import AdminRegionPoliciesPanel from "./admin/AdminRegionPoliciesPanel";
import {
  Shield,
  Check,
  X,
  Trash2,
  RefreshCw,
  BarChart3,
  Image as ImageIcon,
  AlertTriangle,
  LogIn,
  Clock,
  Users,
  Heart,
  MessageCircle,
  ExternalLink,
  MapPin,
  Filter,
  MessageSquare,
  EyeOff,
  Eye,
  Globe,
  SlidersHorizontal,
} from "lucide-react";
import { Lettering } from "../types";

interface AdminStats {
  total_uploads: number;
  pending_approvals: number;
  approved: number;
  rejected: number;
  total_cities: number;
  total_likes: number;
  total_comments: number;
}

interface AdminFilterPreset {
  id: string;
  name: string;
  type: "queue" | "comments";
  payload: Record<string, unknown>;
}

const ADMIN_PRESETS_STORAGE_KEY = "ttl_admin_filter_presets_v1";

const AdminPanel: React.FC<{ onClose: () => void }> = ({ onClose }) => {
  const { addToast } = useToastStore();
  const [token, setToken] = useState<string | null>(() =>
    sessionStorage.getItem(ADMIN_SESSION_KEY),
  );
  const [tab, setTab] = useState<
    "queue" | "reports" | "comments" | "cities" | "regions" | "stats"
  >("queue");
  const [items, setItems] = useState<Lettering[]>([]);
  const [queueTotal, setQueueTotal] = useState(0);
  const [queueLimit, setQueueLimit] = useState(25);
  const [queueOffset, setQueueOffset] = useState(0);
  const [selectedQueueIds, setSelectedQueueIds] = useState<Set<string>>(
    new Set(),
  );
  const [commentItems, setCommentItems] = useState<AdminCommentItem[]>([]);
  const [commentTotal, setCommentTotal] = useState(0);
  const [commentLimit, setCommentLimit] = useState(25);
  const [commentOffset, setCommentOffset] = useState(0);
  const [selectedCommentIds, setSelectedCommentIds] = useState<Set<string>>(
    new Set(),
  );
  const [presets, setPresets] = useState<AdminFilterPreset[]>(() => {
    const raw = localStorage.getItem(ADMIN_PRESETS_STORAGE_KEY);
    if (!raw) return [];
    try {
      const parsed = JSON.parse(raw) as AdminFilterPreset[];
      return Array.isArray(parsed) ? parsed : [];
    } catch {
      return [];
    }
  });
  const [stats, setStats] = useState<AdminStats | null>(null);
  const [statusFilter, setStatusFilter] = useState("PENDING");
  const [commentStatusFilter, setCommentStatusFilter] = useState<
    "ALL" | "VISIBLE" | "HIDDEN"
  >("ALL");
  const [commentQuery, setCommentQuery] = useState("");
  const [reviewOnly, setReviewOnly] = useState(false);
  const [minScore, setMinScore] = useState<number>(0);
  const [commentSort, setCommentSort] = useState<
    "priority" | "newest" | "score"
  >("priority");
  const [loading, setLoading] = useState(false);
  const [actionId, setActionId] = useState<string | null>(null);
  const [commentActionId, setCommentActionId] = useState<string | null>(null);
  const [loginData, setLoginData] = useState({ email: "", password: "" });

  const fetchStats = useCallback(async () => {
    if (!token) return;
    try {
      const data = await api.adminGetStats();
      setStats(data);
    } catch {
      console.error("Stats synchronization failed");
    }
  }, [token]);

  const fetchQueue = useCallback(async () => {
    if (!token) return;
    setLoading(true);
    try {
      const status = tab === "reports" ? "REPORTED" : statusFilter;
      const data = await api.adminGetQueue({
        status,
        limit: queueLimit,
        offset: queueOffset,
      });
      setItems(data.items || []);
      setQueueTotal(data.total || 0);
      setSelectedQueueIds(new Set());
    } catch (err) {
      if ((err as Error).message.includes("401")) {
        sessionStorage.removeItem(ADMIN_SESSION_KEY);
        setToken(null);
        addToast("Session expired", "error");
      } else {
        addToast("Failed to fetch queue", "error");
      }
    } finally {
      setLoading(false);
    }
  }, [tab, statusFilter, token, addToast, queueLimit, queueOffset]);

  const fetchComments = useCallback(async () => {
    if (!token) return;
    setLoading(true);
    try {
      const data = await api.adminGetComments({
        status: commentStatusFilter,
        q: commentQuery.trim() || undefined,
        needs_review: reviewOnly ? true : undefined,
        min_score: minScore > 0 ? minScore : undefined,
        sort: commentSort,
        limit: commentLimit,
        offset: commentOffset,
      });
      setCommentItems(data.items || []);
      setCommentTotal(data.total || 0);
      setSelectedCommentIds(new Set());
    } catch (err) {
      if ((err as Error).message.includes("401")) {
        sessionStorage.removeItem(ADMIN_SESSION_KEY);
        setToken(null);
        addToast("Session expired", "error");
      } else {
        addToast("Failed to fetch comments", "error");
      }
    } finally {
      setLoading(false);
    }
  }, [
    token,
    commentStatusFilter,
    commentQuery,
    reviewOnly,
    minScore,
    commentSort,
    commentLimit,
    commentOffset,
    addToast,
  ]);

  useEffect(() => {
    if (token) {
      if (tab === "comments") {
        fetchComments();
      } else if (tab === "queue" || tab === "reports") {
        fetchQueue();
      }
      fetchStats();
    }
  }, [token, tab, statusFilter, fetchQueue, fetchComments, fetchStats]);

  useEffect(() => {
    localStorage.setItem(ADMIN_PRESETS_STORAGE_KEY, JSON.stringify(presets));
  }, [presets]);

  useEffect(() => {
    setQueueOffset(0);
  }, [tab, statusFilter, queueLimit]);

  useEffect(() => {
    setCommentOffset(0);
  }, [
    commentStatusFilter,
    commentQuery,
    reviewOnly,
    minScore,
    commentSort,
    commentLimit,
  ]);

  const handleLogin = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    try {
      await api.adminLogin(loginData.email, loginData.password);
      setToken(sessionStorage.getItem(ADMIN_SESSION_KEY));
      addToast("Admin Access Granted", "success");
    } catch (err) {
      addToast((err as Error).message || "Credentials invalid", "error");
    } finally {
      setLoading(false);
    }
  };

  const performAction = async (
    id: string,
    action: "approve" | "reject" | "delete" | "keep",
  ) => {
    if (!token) return;

    let reason: string | null = null;
    if (action === "reject") {
      reason = window.prompt("Reason for rejection:");
      if (reason === null) return;
    }

    if (action === "delete" && !window.confirm("Purge artifact from database?"))
      return;

    setActionId(id);
    try {
      if (action === "approve") await api.adminApprove(id);
      else if (action === "keep") await api.adminClearReports(id);
      else if (action === "reject")
        await api.adminReject(id, reason || "Administrative rejection");
      else if (action === "delete") await api.adminDelete(id);

      addToast(
        `Artifact ${action === "keep" ? "cleared" : action + "ed"}`,
        "success",
      );
      setItems((prev) => prev.filter((i) => i.id !== id));
      fetchStats();
    } catch (e) {
      addToast((e as Error).message || "Action failed", "error");
    } finally {
      setActionId(null);
    }
  };

  const performCommentAction = async (
    id: string,
    action: "hide" | "restore" | "delete",
  ) => {
    if (!token) return;

    let reason: string | undefined;
    if (action === "hide") {
      const input = window.prompt("Reason for hiding this comment?");
      if (input === null) return;
      reason = input.trim() || "Hidden by moderation";
    }
    if (
      action === "delete" &&
      !window.confirm("Delete this comment permanently?")
    ) {
      return;
    }

    setCommentActionId(id);
    try {
      if (action === "hide") {
        await api.adminHideComment(id, reason);
      } else if (action === "restore") {
        await api.adminRestoreComment(id);
      } else {
        await api.adminDeleteComment(id);
      }
      addToast(`Comment ${action}d`, "success");
      setCommentItems((prev) => prev.filter((c) => c.id !== id));
      fetchComments();
      fetchStats();
    } catch (e) {
      addToast((e as Error).message || "Comment action failed", "error");
    } finally {
      setCommentActionId(null);
    }
  };

  const toggleQueueSelection = (id: string) => {
    setSelectedQueueIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const toggleCommentSelection = (id: string) => {
    setSelectedCommentIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const selectAllQueueOnPage = (checked: boolean) => {
    setSelectedQueueIds(checked ? new Set(items.map((i) => i.id)) : new Set());
  };

  const selectAllCommentsOnPage = (checked: boolean) => {
    setSelectedCommentIds(
      checked ? new Set(commentItems.map((i) => i.id)) : new Set(),
    );
  };

  const bulkQueueAction = async (
    action: "approve" | "reject" | "delete" | "keep",
  ) => {
    if (selectedQueueIds.size === 0) {
      addToast("Select at least one artifact", "warning");
      return;
    }

    let reason: string | undefined;
    if (action === "reject") {
      const input = window.prompt("Reason for bulk rejection:");
      if (input === null) return;
      reason = input.trim() || "Administrative rejection";
    }
    if (
      action === "delete" &&
      !window.confirm("Delete selected artifacts permanently?")
    ) {
      return;
    }

    try {
      const result = await api.adminBulkLetterings({
        ids: Array.from(selectedQueueIds),
        action,
        reason,
      });
      addToast(
        `Bulk ${action}: ${result.processed} processed, ${result.failed} failed`,
        result.failed > 0 ? "warning" : "success",
      );
      setSelectedQueueIds(new Set());
      await fetchQueue();
      fetchStats();
    } catch (err) {
      addToast((err as Error).message || "Bulk action failed", "error");
    }
  };

  const bulkCommentAction = async (action: "hide" | "restore" | "delete") => {
    if (selectedCommentIds.size === 0) {
      addToast("Select at least one comment", "warning");
      return;
    }

    let reason: string | undefined;
    if (action === "hide") {
      const input = window.prompt("Reason for hiding selected comments:");
      if (input === null) return;
      reason = input.trim() || "Hidden by moderation";
    }
    if (
      action === "delete" &&
      !window.confirm("Delete selected comments permanently?")
    ) {
      return;
    }

    try {
      const result = await api.adminBulkComments({
        ids: Array.from(selectedCommentIds),
        action,
        reason,
      });
      addToast(
        `Bulk ${action}: ${result.processed} processed, ${result.failed} failed`,
        result.failed > 0 ? "warning" : "success",
      );
      setSelectedCommentIds(new Set());
      await fetchComments();
      fetchStats();
    } catch (err) {
      addToast((err as Error).message || "Bulk action failed", "error");
    }
  };

  const saveCurrentPreset = (type: "queue" | "comments") => {
    const name = window.prompt("Preset name:");
    if (!name) return;

    const preset: AdminFilterPreset =
      type === "queue"
        ? {
            id: crypto.randomUUID(),
            name: name.trim(),
            type,
            payload: {
              tab,
              statusFilter,
              queueLimit,
            },
          }
        : {
            id: crypto.randomUUID(),
            name: name.trim(),
            type,
            payload: {
              commentStatusFilter,
              commentQuery,
              reviewOnly,
              minScore,
              commentSort,
              commentLimit,
            },
          };

    setPresets((prev) => [preset, ...prev].slice(0, 20));
    addToast("Preset saved", "success");
  };

  const applyPreset = (preset: AdminFilterPreset) => {
    if (preset.type === "queue") {
      const nextTab = String(preset.payload.tab || "queue");
      setTab(nextTab === "reports" ? "reports" : "queue");
      setStatusFilter(String(preset.payload.statusFilter || "PENDING"));
      setQueueLimit(Number(preset.payload.queueLimit || 25));
      setQueueOffset(0);
    } else {
      setTab("comments");
      setCommentStatusFilter(
        String(preset.payload.commentStatusFilter || "ALL") as
          | "ALL"
          | "VISIBLE"
          | "HIDDEN",
      );
      setCommentQuery(String(preset.payload.commentQuery || ""));
      setReviewOnly(Boolean(preset.payload.reviewOnly));
      setMinScore(Number(preset.payload.minScore || 0));
      setCommentSort(
        String(preset.payload.commentSort || "priority") as
          | "priority"
          | "newest"
          | "score",
      );
      setCommentLimit(Number(preset.payload.commentLimit || 25));
      setCommentOffset(0);
    }
  };

  const deletePreset = (id: string) => {
    setPresets((prev) => prev.filter((preset) => preset.id !== id));
  };

  if (!token) {
    return (
      <>
        <Helmet>
          <title>Admin Login | Through Your Letters</title>
        </Helmet>
        <div className="max-w-md mx-auto pt-20 space-y-8 animate-in">
          <div className="flex items-center gap-4">
            <Shield className="text-[#cc543a]" size={32} />
            <h1 className="text-4xl font-black uppercase tracking-tighter">
              Admin Portal
            </h1>
          </div>
          <form
            onSubmit={handleLogin}
            className="space-y-4 bg-white p-10 border-4 border-black brutalist-shadow"
          >
            <input
              type="email"
              placeholder="Email"
              className="w-full border-2 border-black p-4 font-black"
              onChange={(e) =>
                setLoginData({ ...loginData, email: e.target.value })
              }
              required
            />
            <input
              type="password"
              placeholder="Password"
              className="w-full border-2 border-black p-4 font-black"
              onChange={(e) =>
                setLoginData({ ...loginData, password: e.target.value })
              }
              required
            />
            <button
              type="submit"
              disabled={loading}
              className="w-full bg-black text-white py-5 font-black uppercase tracking-widest flex items-center justify-center gap-3 active:translate-y-1 transition-all"
            >
              {loading ? (
                <RefreshCw className="animate-spin" />
              ) : (
                <LogIn size={20} />
              )}{" "}
              Initialize Node
            </button>
          </form>
        </div>
      </>
    );
  }

  return (
    <>
      <Helmet>
        <title>Admin Dashboard | Through Your Letters</title>
      </Helmet>
      <div className="max-w-6xl mx-auto space-y-10 pb-32 animate-in">
        <div className="flex flex-col md:flex-row justify-between items-start md:items-center border-b-4 border-black pb-8 gap-6">
          <div className="flex items-center gap-4">
            <div className="w-10 h-10 bg-black flex items-center justify-center">
              <Shield className="text-[#cc543a]" size={20} />
            </div>
            <h1 className="text-3xl font-black uppercase tracking-tighter">
              Curator Control
            </h1>
          </div>
          <div className="flex gap-3">
            <button
              onClick={onClose}
              className="bg-black text-white px-6 py-2 text-[10px] font-black uppercase hover:bg-[#cc543a] transition-all"
            >
              Exit Dashboard
            </button>
          </div>
        </div>

        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <StatCard
            icon={<ImageIcon size={18} />}
            label="Total Artifacts"
            value={stats?.total_uploads || 0}
          />
          <StatCard
            icon={<Clock size={18} />}
            label="Pending Review"
            value={stats?.pending_approvals || 0}
            color="text-[#cc543a]"
          />
          <StatCard
            icon={<Heart size={18} />}
            label="Archive Likes"
            value={stats?.total_likes || 0}
          />
          <StatCard
            icon={<MessageCircle size={18} />}
            label="Notes/Comments"
            value={stats?.total_comments || 0}
          />
        </div>

        <div className="flex border-4 border-black bg-white sticky top-0 z-20 brutalist-shadow-sm">
          <button
            onClick={() => setTab("queue")}
            className={`flex-1 py-5 font-black uppercase text-xs flex items-center justify-center gap-2 ${tab === "queue" ? "bg-black text-white" : "hover:bg-slate-50"}`}
          >
            <Filter size={16} /> Moderation
          </button>
          <button
            onClick={() => setTab("reports")}
            className={`flex-1 py-5 font-black uppercase text-xs border-l-4 border-black flex items-center justify-center gap-2 ${tab === "reports" ? "bg-[#cc543a] text-white" : "hover:bg-slate-50"}`}
          >
            <AlertTriangle size={16} /> Flags
          </button>
          <button
            onClick={() => setTab("comments")}
            className={`flex-1 py-5 font-black uppercase text-xs border-l-4 border-black flex items-center justify-center gap-2 ${tab === "comments" ? "bg-black text-white" : "hover:bg-slate-50"}`}
          >
            <MessageSquare size={16} /> Comments
          </button>
          <button
            onClick={() => setTab("cities")}
            className={`flex-1 py-5 font-black uppercase text-xs border-l-4 border-black flex items-center justify-center gap-2 ${tab === "cities" ? "bg-black text-white" : "hover:bg-slate-50"}`}
          >
            <Globe size={16} /> Cities
          </button>
          <button
            onClick={() => setTab("regions")}
            className={`flex-1 py-5 font-black uppercase text-xs border-l-4 border-black flex items-center justify-center gap-2 ${tab === "regions" ? "bg-black text-white" : "hover:bg-slate-50"}`}
          >
            <SlidersHorizontal size={16} /> Regions
          </button>
          <button
            onClick={() => setTab("stats")}
            className={`flex-1 py-5 font-black uppercase text-xs border-l-4 border-black flex items-center justify-center gap-2 ${tab === "stats" ? "bg-black text-white" : "hover:bg-slate-50"}`}
          >
            <BarChart3 size={16} /> Activity
          </button>
        </div>

        {(tab === "queue" || tab === "reports") && (
          <div className="space-y-8">
            <div className="space-y-4 bg-slate-50 p-4 border-2 border-black">
              <div className="flex flex-wrap justify-between items-center gap-3">
                <div className="flex gap-4 items-center">
                  <span className="text-[10px] font-black uppercase text-slate-400">
                    Queue Filter:
                  </span>
                  {["PENDING", "APPROVED", "REJECTED"].map((s) => (
                    <button
                      key={s}
                      onClick={() => setStatusFilter(s)}
                      className={`px-3 py-1 text-[9px] font-black uppercase border-2 border-black ${statusFilter === s ? "bg-black text-white" : "bg-white"}`}
                    >
                      {s}
                    </button>
                  ))}
                </div>
                <div className="flex items-center gap-2">
                  <select
                    value={queueLimit}
                    onChange={(e) => setQueueLimit(Number(e.target.value))}
                    className="border-2 border-black px-2 py-1 text-[10px] font-black uppercase"
                  >
                    {[10, 25, 50, 100].map((size) => (
                      <option key={size} value={size}>
                        {size}/page
                      </option>
                    ))}
                  </select>
                  <button
                    onClick={fetchQueue}
                    className="text-[#cc543a] hover:rotate-180 transition-transform"
                    title="Refresh"
                  >
                    <RefreshCw size={20} />
                  </button>
                </div>
              </div>

              <div className="flex flex-wrap gap-2">
                <button
                  onClick={() => saveCurrentPreset("queue")}
                  className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase bg-white hover:bg-black hover:text-white transition-colors"
                >
                  Save Filter Preset
                </button>
                {presets
                  .filter((preset) => preset.type === "queue")
                  .slice(0, 4)
                  .map((preset) => (
                    <div key={preset.id} className="flex">
                      <button
                        onClick={() => applyPreset(preset)}
                        className="border-2 border-black border-r-0 px-3 py-1 text-[9px] font-black uppercase bg-white hover:bg-slate-100 transition-colors"
                      >
                        {preset.name}
                      </button>
                      <button
                        onClick={() => deletePreset(preset.id)}
                        className="border-2 border-black px-2 py-1 text-[9px] font-black uppercase text-red-600 bg-white hover:bg-red-50 transition-colors"
                      >
                        X
                      </button>
                    </div>
                  ))}
              </div>

              <div className="flex flex-wrap items-center gap-2 border-t-2 border-black/10 pt-3">
                <label className="inline-flex items-center gap-2 text-[10px] font-black uppercase">
                  <input
                    type="checkbox"
                    checked={
                      items.length > 0 && selectedQueueIds.size === items.length
                    }
                    onChange={(e) => selectAllQueueOnPage(e.target.checked)}
                  />
                  Select Page
                </label>
                <button
                  onClick={() =>
                    void bulkQueueAction(tab === "reports" ? "keep" : "approve")
                  }
                  disabled={selectedQueueIds.size === 0}
                  className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase bg-white hover:bg-green-50 disabled:opacity-40"
                >
                  {tab === "reports" ? "Bulk Clear Flags" : "Bulk Approve"}
                </button>
                {tab !== "reports" && (
                  <button
                    onClick={() => void bulkQueueAction("reject")}
                    disabled={selectedQueueIds.size === 0}
                    className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase bg-white hover:bg-yellow-50 disabled:opacity-40"
                  >
                    Bulk Reject
                  </button>
                )}
                <button
                  onClick={() => void bulkQueueAction("delete")}
                  disabled={selectedQueueIds.size === 0}
                  className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase bg-white text-red-600 hover:bg-red-50 disabled:opacity-40"
                >
                  Bulk Delete
                </button>
              </div>

              <div className="flex items-center justify-between border-t-2 border-black/10 pt-3">
                <p className="text-[10px] font-black uppercase text-slate-500">
                  {queueOffset + 1}-
                  {Math.min(queueOffset + queueLimit, queueTotal)} of{" "}
                  {queueTotal}
                </p>
                <div className="flex gap-2">
                  <button
                    onClick={() =>
                      setQueueOffset(Math.max(queueOffset - queueLimit, 0))
                    }
                    disabled={queueOffset === 0}
                    className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase disabled:opacity-40"
                  >
                    Prev
                  </button>
                  <button
                    onClick={() => setQueueOffset(queueOffset + queueLimit)}
                    disabled={queueOffset + queueLimit >= queueTotal}
                    className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase disabled:opacity-40"
                  >
                    Next
                  </button>
                </div>
              </div>
            </div>

            <div className="grid gap-6">
              {loading ? (
                <RefreshCw
                  className="animate-spin mx-auto text-[#cc543a]"
                  size={40}
                />
              ) : items.length === 0 ? (
                <div className="text-center py-32 border-4 border-dashed border-black/10 font-black uppercase text-slate-300">
                  Nothing here requires attention
                </div>
              ) : (
                items.map((item) => (
                  <ModerationCard
                    key={item.id}
                    item={item}
                    isProcessing={actionId === item.id}
                    selected={selectedQueueIds.has(item.id)}
                    onToggleSelected={() => toggleQueueSelection(item.id)}
                    onApprove={() => performAction(item.id, "approve")}
                    onReject={() => performAction(item.id, "reject")}
                    onDelete={() => performAction(item.id, "delete")}
                  />
                ))
              )}
            </div>
          </div>
        )}

        {tab === "reports" && (
          <div className="space-y-8">
            <div className="bg-yellow-50 border-4 border-yellow-600 p-6 flex items-center gap-4">
              <AlertTriangle className="text-yellow-600" size={32} />
              <div>
                <h2 className="font-black uppercase text-lg text-yellow-900 leading-none">
                  Priority Content flagged
                </h2>
                <p className="text-[10px] font-bold text-yellow-700 mt-1 uppercase tracking-widest">
                  Review reports and decide whether to retain or purge
                  artifacts.
                </p>
              </div>
            </div>
            <div className="bg-white border-4 border-black p-4 space-y-3">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex items-center gap-2">
                  <label className="inline-flex items-center gap-2 text-[10px] font-black uppercase">
                    <input
                      type="checkbox"
                      checked={
                        items.length > 0 &&
                        selectedQueueIds.size === items.length
                      }
                      onChange={(e) => selectAllQueueOnPage(e.target.checked)}
                    />
                    Select Page
                  </label>
                  <button
                    onClick={() => void bulkQueueAction("keep")}
                    disabled={selectedQueueIds.size === 0}
                    className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase hover:bg-green-50 disabled:opacity-40"
                  >
                    Bulk Clear Flags
                  </button>
                  <button
                    onClick={() => void bulkQueueAction("delete")}
                    disabled={selectedQueueIds.size === 0}
                    className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase text-red-600 hover:bg-red-50 disabled:opacity-40"
                  >
                    Bulk Delete
                  </button>
                </div>
                <div className="flex items-center gap-2">
                  <select
                    value={queueLimit}
                    onChange={(e) => setQueueLimit(Number(e.target.value))}
                    className="border-2 border-black px-2 py-1 text-[10px] font-black uppercase"
                  >
                    {[10, 25, 50, 100].map((size) => (
                      <option key={size} value={size}>
                        {size}/page
                      </option>
                    ))}
                  </select>
                  <button
                    onClick={fetchQueue}
                    className="text-[#cc543a] hover:rotate-180 transition-transform"
                  >
                    <RefreshCw size={18} />
                  </button>
                </div>
              </div>
              <div className="flex items-center justify-between border-t-2 border-black/10 pt-3">
                <p className="text-[10px] font-black uppercase text-slate-500">
                  {queueOffset + 1}-
                  {Math.min(queueOffset + queueLimit, queueTotal)} of{" "}
                  {queueTotal}
                </p>
                <div className="flex gap-2">
                  <button
                    onClick={() =>
                      setQueueOffset(Math.max(queueOffset - queueLimit, 0))
                    }
                    disabled={queueOffset === 0}
                    className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase disabled:opacity-40"
                  >
                    Prev
                  </button>
                  <button
                    onClick={() => setQueueOffset(queueOffset + queueLimit)}
                    disabled={queueOffset + queueLimit >= queueTotal}
                    className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase disabled:opacity-40"
                  >
                    Next
                  </button>
                </div>
              </div>
            </div>
            <div className="grid gap-6">
              {loading ? (
                <RefreshCw
                  className="animate-spin mx-auto text-[#cc543a]"
                  size={40}
                />
              ) : items.length === 0 ? (
                <div className="text-center py-32 border-4 border-dashed border-black/10 font-black uppercase text-slate-300">
                  No active reports
                </div>
              ) : (
                items.map((item) => (
                  <ModerationCard
                    key={item.id}
                    item={item}
                    isProcessing={actionId === item.id}
                    selected={selectedQueueIds.has(item.id)}
                    onToggleSelected={() => toggleQueueSelection(item.id)}
                    isReported
                    onApprove={() => performAction(item.id, "keep")}
                    onDelete={() => performAction(item.id, "delete")}
                  />
                ))
              )}
            </div>
          </div>
        )}

        {tab === "comments" && (
          <div className="space-y-8">
            <div className="bg-white border-4 border-black p-4 space-y-4">
              <div className="flex flex-col md:flex-row gap-4 md:items-center md:justify-between">
                <div className="flex items-center gap-2">
                  {(["ALL", "VISIBLE", "HIDDEN"] as const).map((s) => (
                    <button
                      key={s}
                      onClick={() => setCommentStatusFilter(s)}
                      className={`px-3 py-1 text-[9px] font-black uppercase border-2 border-black ${commentStatusFilter === s ? "bg-black text-white" : "bg-white"}`}
                    >
                      {s}
                    </button>
                  ))}
                </div>
                <div className="flex gap-2">
                  <select
                    value={commentLimit}
                    onChange={(e) => setCommentLimit(Number(e.target.value))}
                    className="border-2 border-black px-3 py-2 text-[10px] font-black uppercase tracking-widest outline-none"
                  >
                    {[10, 25, 50, 100].map((size) => (
                      <option key={size} value={size}>
                        {size}/page
                      </option>
                    ))}
                  </select>
                  <input
                    value={commentQuery}
                    onChange={(e) => setCommentQuery(e.target.value)}
                    placeholder="Search comment or user..."
                    className="border-2 border-black px-3 py-2 text-[10px] font-bold uppercase tracking-widest outline-none min-w-[220px]"
                  />
                  <button
                    onClick={fetchComments}
                    className="bg-black text-white px-4 py-2 text-[10px] font-black uppercase hover:bg-[#cc543a] transition-colors"
                  >
                    Refresh
                  </button>
                </div>
              </div>
              <div className="flex flex-wrap gap-2 border-t-2 border-black/10 pt-3">
                <button
                  onClick={() => saveCurrentPreset("comments")}
                  className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase hover:bg-black hover:text-white transition-colors"
                >
                  Save Filter Preset
                </button>
                {presets
                  .filter((preset) => preset.type === "comments")
                  .slice(0, 4)
                  .map((preset) => (
                    <div key={preset.id} className="flex">
                      <button
                        onClick={() => applyPreset(preset)}
                        className="border-2 border-black border-r-0 px-3 py-1 text-[9px] font-black uppercase bg-white hover:bg-slate-100 transition-colors"
                      >
                        {preset.name}
                      </button>
                      <button
                        onClick={() => deletePreset(preset.id)}
                        className="border-2 border-black px-2 py-1 text-[9px] font-black uppercase text-red-600 bg-white hover:bg-red-50 transition-colors"
                      >
                        X
                      </button>
                    </div>
                  ))}
              </div>
              <div className="flex flex-col md:flex-row gap-3 md:items-center md:justify-between border-t-2 border-black/10 pt-3">
                <div className="flex items-center gap-3">
                  <label className="flex items-center gap-2 text-[10px] font-black uppercase tracking-widest text-slate-500">
                    <input
                      type="checkbox"
                      checked={reviewOnly}
                      onChange={(e) => setReviewOnly(e.target.checked)}
                    />
                    Needs Review Only
                  </label>
                  <label className="flex items-center gap-2 text-[10px] font-black uppercase tracking-widest text-slate-500">
                    Min Score
                    <input
                      type="number"
                      min={0}
                      max={100}
                      value={minScore}
                      onChange={(e) => setMinScore(Number(e.target.value) || 0)}
                      className="w-16 border-2 border-black px-2 py-1 text-[10px] font-black"
                    />
                  </label>
                </div>
                <label className="flex items-center gap-2 text-[10px] font-black uppercase tracking-widest text-slate-500">
                  Sort
                  <select
                    value={commentSort}
                    onChange={(e) =>
                      setCommentSort(
                        e.target.value as "priority" | "newest" | "score",
                      )
                    }
                    className="border-2 border-black px-2 py-1 text-[10px] font-black"
                  >
                    <option value="priority">Priority</option>
                    <option value="score">Score</option>
                    <option value="newest">Newest</option>
                  </select>
                </label>
              </div>
              <div className="flex flex-wrap items-center gap-2 border-t-2 border-black/10 pt-3">
                <label className="inline-flex items-center gap-2 text-[10px] font-black uppercase">
                  <input
                    type="checkbox"
                    checked={
                      commentItems.length > 0 &&
                      selectedCommentIds.size === commentItems.length
                    }
                    onChange={(e) => selectAllCommentsOnPage(e.target.checked)}
                  />
                  Select Page
                </label>
                <button
                  onClick={() => void bulkCommentAction("hide")}
                  disabled={selectedCommentIds.size === 0}
                  className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase hover:bg-yellow-50 disabled:opacity-40"
                >
                  Bulk Hide
                </button>
                <button
                  onClick={() => void bulkCommentAction("restore")}
                  disabled={selectedCommentIds.size === 0}
                  className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase hover:bg-green-50 disabled:opacity-40"
                >
                  Bulk Restore
                </button>
                <button
                  onClick={() => void bulkCommentAction("delete")}
                  disabled={selectedCommentIds.size === 0}
                  className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase text-red-600 hover:bg-red-50 disabled:opacity-40"
                >
                  Bulk Delete
                </button>
              </div>
              <div className="flex items-center justify-between border-t-2 border-black/10 pt-3">
                <p className="text-[10px] font-black uppercase text-slate-500">
                  {commentOffset + 1}-
                  {Math.min(commentOffset + commentLimit, commentTotal)} of{" "}
                  {commentTotal}
                </p>
                <div className="flex gap-2">
                  <button
                    onClick={() =>
                      setCommentOffset(
                        Math.max(commentOffset - commentLimit, 0),
                      )
                    }
                    disabled={commentOffset === 0}
                    className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase disabled:opacity-40"
                  >
                    Prev
                  </button>
                  <button
                    onClick={() =>
                      setCommentOffset(commentOffset + commentLimit)
                    }
                    disabled={commentOffset + commentLimit >= commentTotal}
                    className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase disabled:opacity-40"
                  >
                    Next
                  </button>
                </div>
              </div>
            </div>

            <div className="grid gap-4">
              {loading ? (
                <RefreshCw
                  className="animate-spin mx-auto text-[#cc543a]"
                  size={40}
                />
              ) : commentItems.length === 0 ? (
                <div className="text-center py-20 border-4 border-dashed border-black/10 font-black uppercase text-slate-300">
                  No comments match current filters
                </div>
              ) : (
                commentItems.map((comment) => (
                  <CommentModerationCard
                    key={comment.id}
                    item={comment}
                    isProcessing={commentActionId === comment.id}
                    selected={selectedCommentIds.has(comment.id)}
                    onToggleSelected={() => toggleCommentSelection(comment.id)}
                    onHide={() => performCommentAction(comment.id, "hide")}
                    onRestore={() =>
                      performCommentAction(comment.id, "restore")
                    }
                    onDelete={() => performCommentAction(comment.id, "delete")}
                  />
                ))
              )}
            </div>
          </div>
        )}

        {tab === "cities" && <AdminCitiesPanel />}
        {tab === "regions" && <AdminRegionPoliciesPanel />}

        {tab === "stats" && stats && (
          <div className="space-y-12 bg-white border-4 border-black p-12 brutalist-shadow">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-20">
              <div className="space-y-8">
                <h3 className="text-4xl font-black uppercase tracking-tighter border-b-4 border-black pb-4">
                  Node Insights
                </h3>
                <div className="space-y-4 font-black uppercase text-sm">
                  <div className="flex justify-between border-b border-black/5 pb-2">
                    <span>Total Discovery Entries</span>
                    <span className="text-[#cc543a]">
                      {stats.total_uploads}
                    </span>
                  </div>
                  <div className="flex justify-between border-b border-black/5 pb-2">
                    <span>Curation Accuracy</span>
                    <span className="text-[#cc543a]">
                      {Math.round(
                        (stats.approved / (stats.total_uploads || 1)) * 100,
                      )}
                      %
                    </span>
                  </div>
                  <div className="flex justify-between border-b border-black/5 pb-2">
                    <span>Unique Contributors</span>
                    <span className="text-[#cc543a]">{stats.total_cities}</span>
                  </div>
                </div>
              </div>
              <div className="bg-slate-50 border-4 border-black p-8 relative">
                <h4 className="text-xl font-black uppercase mb-6 tracking-tighter">
                  Infrastructure
                </h4>
                <div className="space-y-4">
                  <HealthIndicator
                    label="PostgreSQL Core"
                    value="Online"
                    color="bg-green-500"
                  />
                  <HealthIndicator
                    label="R2 File Storage"
                    value="Stable"
                    color="bg-green-500"
                  />
                  <HealthIndicator
                    label="ML Processing Node"
                    value="Idle"
                    color="bg-blue-500"
                  />
                </div>
              </div>
            </div>
          </div>
        )}
      </div>
    </>
  );
};

const StatCard = ({ icon, label, value, color = "text-black" }: any) => (
  <div className="bg-white border-4 border-black p-6 brutalist-shadow-sm space-y-4">
    <div className="text-slate-400">{icon}</div>
    <div>
      <p className={`text-4xl font-black tracking-tighter ${color}`}>{value}</p>
      <p className="text-[10px] font-black uppercase text-slate-400 tracking-widest">
        {label}
      </p>
    </div>
  </div>
);

const ModerationCard = ({
  item,
  onApprove,
  onReject,
  onDelete,
  isProcessing,
  isReported,
  selected,
  onToggleSelected,
}: any) => (
  <div className="bg-white border-4 border-black p-6 flex flex-col md:flex-row gap-8 transition-all hover:bg-slate-50">
    <div className="w-full md:w-56 h-56 flex-shrink-0 border-2 border-black bg-slate-100 overflow-hidden relative group">
      <label className="absolute top-2 left-2 z-10 bg-white/90 border border-black px-1 py-0.5">
        <input type="checkbox" checked={selected} onChange={onToggleSelected} />
      </label>
      <img
        src={item.image_url}
        className="w-full h-full object-cover transition-transform group-hover:scale-105"
        alt="Artifact"
      />
      <a
        href={item.image_url}
        target="_blank"
        rel="noreferrer"
        className="absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity"
      >
        <ExternalLink className="text-white" size={24} />
      </a>
    </div>
    <div className="flex-1 space-y-6">
      <div className="flex justify-between items-start gap-4">
        <div className="space-y-1">
          <p className="text-[10px] font-black uppercase text-[#cc543a] flex items-center gap-2">
            <MapPin size={10} /> {item.pin_code} // <Users size={10} />{" "}
            {item.contributor_tag}
          </p>
          <h3 className="text-2xl font-black uppercase tracking-tighter break-words">
            {item.detected_text || "Awaiting Scan"}
          </h3>
          <p className="text-[9px] font-bold text-slate-400 uppercase">
            {new Date(item.created_at).toLocaleString()}
          </p>
        </div>
        {isReported && (
          <div className="bg-red-50 border-2 border-red-600 px-4 py-2 flex items-center gap-2 text-red-700 font-black text-[10px] uppercase">
            <AlertTriangle size={14} /> {item.report_count || 1} Flags
          </div>
        )}
      </div>

      <div className="grid grid-cols-2 gap-6">
        <div className="space-y-2">
          <p className="text-[9px] font-black uppercase text-slate-400 tracking-widest">
            Description
          </p>
          <p className="text-sm font-medium text-slate-700 leading-relaxed italic break-words line-clamp-3">
            "{item.description || "No context provided."}"
          </p>
        </div>
        <div className="space-y-2 border-l-2 border-slate-100 pl-6">
          <p className="text-[9px] font-black uppercase text-slate-400 tracking-widest">
            Signals
          </p>
          <div className="flex gap-4">
            <span className="flex items-center gap-1 text-[10px] font-black">
              <Heart size={12} /> {item.likes_count || 0}
            </span>
            <span className="flex items-center gap-1 text-[10px] font-black">
              <MessageCircle size={12} /> {item.comments_count || 0}
            </span>
          </div>
          {item.ml_metadata && (
            <div className="flex flex-wrap gap-2 mt-2">
              <span className="bg-slate-100 px-2 py-0.5 text-[8px] font-black uppercase border border-black">
                {item.ml_metadata.style}
              </span>
              <span className="bg-slate-100 px-2 py-0.5 text-[8px] font-black uppercase border border-black">
                {item.ml_metadata.script}
              </span>
            </div>
          )}
        </div>
      </div>

      {item.cultural_context && (
        <div className="bg-slate-50 p-4 border-l-4 border-[#2d5a27] space-y-1">
          <p className="text-[10px] font-black uppercase text-[#2d5a27] tracking-widest">
            Neighborhood History (Wikipedia)
          </p>
          <p className="text-sm text-slate-700 leading-relaxed italic line-clamp-4">
            {item.cultural_context}
          </p>
        </div>
      )}

      {isReported && item.report_reasons && (
        <div className="bg-red-50/50 p-4 border-l-4 border-red-600 space-y-1">
          <p className="text-[10px] font-black uppercase text-red-600 tracking-widest">
            User Complaints:
          </p>
          {item.report_reasons.map((r: string, i: number) => (
            <p key={i} className="text-sm font-bold text-red-900">
              - {r}
            </p>
          ))}
        </div>
      )}

      <div className="flex gap-4 pt-2">
        <button
          disabled={isProcessing}
          onClick={onApprove}
          className="flex-1 bg-black text-white py-4 font-black uppercase text-[11px] tracking-widest flex items-center justify-center gap-2 hover:bg-green-600 transition-all disabled:opacity-50"
        >
          {isProcessing ? (
            <RefreshCw className="animate-spin" size={16} />
          ) : (
            <Check size={18} />
          )}
          {isReported ? "Clear Flags" : "Approve"}
        </button>
        {!isReported && (
          <button
            disabled={isProcessing}
            onClick={onReject}
            className="flex-1 border-2 border-black py-4 font-black uppercase text-[11px] flex items-center justify-center gap-2 hover:bg-red-50 disabled:opacity-50 transition-all"
          >
            <X size={18} /> Reject
          </button>
        )}
        <button
          disabled={isProcessing}
          onClick={onDelete}
          className="px-8 border-2 border-black py-4 font-black uppercase text-[11px] text-red-600 hover:bg-red-600 hover:text-white disabled:opacity-50 transition-all"
        >
          <Trash2 size={18} />
        </button>
      </div>
    </div>
  </div>
);

const CommentModerationCard = ({
  item,
  isProcessing,
  selected,
  onToggleSelected,
  onHide,
  onRestore,
  onDelete,
}: {
  item: AdminCommentItem;
  isProcessing: boolean;
  selected: boolean;
  onToggleSelected: () => void;
  onHide: () => void;
  onRestore: () => void;
  onDelete: () => void;
}) => (
  <div className="border-4 border-black bg-white p-5 md:p-6 space-y-4">
    <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-3">
      <div className="space-y-1">
        <p className="text-[10px] font-black uppercase text-[#cc543a] tracking-widest">
          {item.commenter_name || "Anonymous"}{" "}
          {item.commenter_email ? `(${item.commenter_email})` : ""}
        </p>
        <p className="text-sm font-bold text-slate-900 break-words">
          {item.content}
        </p>
        <p className="text-[9px] font-bold uppercase text-slate-400">
          {new Date(item.created_at).toLocaleString()}
        </p>
      </div>
      <div className="flex items-center gap-2">
        <label className="border-2 border-black px-2 py-1 bg-white">
          <input
            type="checkbox"
            checked={selected}
            onChange={onToggleSelected}
          />
        </label>
        <span
          className={`px-2 py-1 text-[9px] font-black uppercase border-2 border-black ${item.status === "HIDDEN" ? "bg-red-50 text-red-700" : "bg-green-50 text-green-700"}`}
        >
          {item.status}
        </span>
      </div>
    </div>

    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      <div className="space-y-1">
        <p className="text-[9px] font-black uppercase text-slate-400 tracking-widest">
          Artifact Context
        </p>
        <p className="text-[10px] font-black uppercase">
          PIN {item.pin_code} // {item.contributor_tag}
        </p>
        <a
          href={item.lettering_image_url}
          target="_blank"
          rel="noreferrer"
          className="inline-flex items-center gap-1 text-[10px] font-black uppercase text-[#cc543a] hover:underline"
        >
          <ExternalLink size={12} />
          Open Artifact
        </a>
      </div>
      <div className="space-y-1">
        <p className="text-[9px] font-black uppercase text-slate-400 tracking-widest">
          Moderation
        </p>
        <p className="text-[10px] font-black uppercase">
          Score {item.moderation_score} / Priority {item.review_priority}
        </p>
        <p className="text-[10px] font-black uppercase text-slate-500">
          {item.auto_flagged
            ? "AUTO FLAGGED"
            : item.needs_review
              ? "REVIEW NEEDED"
              : "NO ACTIVE RISK SIGNAL"}
        </p>
        {item.moderation_flags?.length > 0 && (
          <div className="flex flex-wrap gap-2 py-1">
            {item.moderation_flags.map((flag) => (
              <span
                key={flag}
                className="border border-black px-1.5 py-0.5 text-[9px] font-black uppercase bg-yellow-50"
              >
                {flag}
              </span>
            ))}
          </div>
        )}
        {item.moderation_reason ? (
          <p className="text-sm font-medium text-slate-700 italic break-words">
            "{item.moderation_reason}"
          </p>
        ) : (
          <p className="text-sm font-medium text-slate-400 italic">
            No moderation reason recorded
          </p>
        )}
        {item.moderated_by && (
          <p className="text-[9px] font-bold uppercase text-slate-400">
            by {item.moderated_by}
          </p>
        )}
      </div>
    </div>

    <div className="flex gap-3 pt-2">
      {item.status === "VISIBLE" ? (
        <button
          disabled={isProcessing}
          onClick={onHide}
          className="flex-1 border-2 border-black py-3 font-black uppercase text-[11px] flex items-center justify-center gap-2 hover:bg-yellow-50 disabled:opacity-50 transition-all"
        >
          {isProcessing ? (
            <RefreshCw size={16} className="animate-spin" />
          ) : (
            <EyeOff size={16} />
          )}
          Hide
        </button>
      ) : (
        <button
          disabled={isProcessing}
          onClick={onRestore}
          className="flex-1 border-2 border-black py-3 font-black uppercase text-[11px] flex items-center justify-center gap-2 hover:bg-green-50 disabled:opacity-50 transition-all"
        >
          {isProcessing ? (
            <RefreshCw size={16} className="animate-spin" />
          ) : (
            <Eye size={16} />
          )}
          Restore
        </button>
      )}
      <button
        disabled={isProcessing}
        onClick={onDelete}
        className="px-8 border-2 border-black py-3 font-black uppercase text-[11px] text-red-600 hover:bg-red-600 hover:text-white disabled:opacity-50 transition-all"
      >
        <Trash2 size={18} />
      </button>
    </div>
  </div>
);

const HealthIndicator = ({ label, value, color }: any) => (
  <div className="flex items-center justify-between border-b border-black/5 pb-2">
    <span className="text-[10px] font-black uppercase text-slate-500">
      {label}
    </span>
    <div className="flex items-center gap-2">
      <span className="text-[10px] font-black uppercase">{value}</span>
      <div className={`w-2 h-2 rounded-full ${color}`}></div>
    </div>
  </div>
);

export default AdminPanel;
