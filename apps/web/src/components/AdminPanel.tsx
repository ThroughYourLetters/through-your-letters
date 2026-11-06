import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
} from "react";
import { Helmet } from "react-helmet-async";
import { ADMIN_SESSION_KEY, api } from "../lib/api";
import { useToastStore } from "../store/useToastStore";
import AdminCitiesPanel from "./admin/AdminCitiesPanel";
import AdminCommentsPanel from "./admin/AdminCommentsPanel";
import AdminJobsPanel from "./admin/AdminJobsPanel";
import AdminLocationsPanel from "./admin/AdminLocationsPanel";
import AdminModerationPanel from "./admin/AdminModerationPanel";
import AdminRegionPoliciesPanel from "./admin/AdminRegionPoliciesPanel";
import AdminStatsPanel from "./admin/AdminStatsPanel";
import {
  useAdminResource,
  type AdminSessionProps,
} from "./admin/useAdminOperations";

const sections = [
  ["queue", "Moderation"],
  ["reports", "Flags"],
  ["comments", "Comments"],
  ["jobs", "Failed jobs"],
  ["cities", "Cities"],
  ["regions", "Regions"],
  ["locations", "Locations"],
  ["stats", "Activity"],
] as const;
type Tab = (typeof sections)[number][0];
const loadStats = (signal: AbortSignal) => api.adminGetStats(signal);

function Dashboard({
  token,
  onSessionExpired,
  onClose,
}: AdminSessionProps & { onClose: () => void }) {
  const [tab, setTab] = useState<Tab>("queue");
  const stats = useAdminResource({ token, onSessionExpired }, loadStats);
  const refreshStats = stats.reload;
  useEffect(() => {
    if (tab === "stats") refreshStats();
  }, [tab, refreshStats]);
  const counts = [
    ["Uploads", stats.data?.total_uploads],
    ["Pending review", stats.data?.pending_approvals],
    ["Likes", stats.data?.total_likes],
    ["Comments", stats.data?.total_comments],
  ] as const;
  return (
    <>
      <Helmet>
        <title>Admin Dashboard | Through Your Letters</title>
      </Helmet>
      <div className="max-w-6xl mx-auto space-y-8 pb-20">
        <div className="flex flex-wrap justify-between items-center gap-4 border-b border-line pb-6">
          <h1 className="font-display text-4xl">Admin</h1>
          <button className="btn-dark" onClick={onClose}>
            Exit dashboard
          </button>
        </div>
        <dl className="grid grid-cols-2 md:grid-cols-4 gap-4">
          {counts.map(([label, value]) => (
            <div key={label} className="card p-5">
              <dt className="text-sm text-muted">{label}</dt>
              <dd className="font-display text-3xl mt-2">
                {value === undefined
                  ? stats.loading
                    ? "…"
                    : "Unavailable"
                  : value.toLocaleString()}
              </dd>
            </div>
          ))}
        </dl>
        {stats.error && (
          <p role="alert">
            Counts could not be loaded.{" "}
            <button className="underline" onClick={stats.reload}>
              Retry counts
            </button>
          </p>
        )}
        <nav
          aria-label="Admin sections"
          className="flex gap-2 overflow-x-auto border border-line rounded-2xl p-2"
        >
          {sections.map(([key, label]) => (
            <button
              key={key}
              aria-pressed={tab === key}
              onClick={() => setTab(key)}
              className={`btn-outline shrink-0 ${tab === key ? "bg-ink text-paper" : ""}`}
            >
              {label}
            </button>
          ))}
        </nav>
        {(tab === "queue" || tab === "reports") && (
          <AdminModerationPanel
            token={token}
            onSessionExpired={onSessionExpired}
            mode={tab}
            onModeChange={setTab}
            onChanged={stats.reload}
          />
        )}
        {tab === "comments" && (
          <AdminCommentsPanel
            token={token}
            onSessionExpired={onSessionExpired}
            onChanged={stats.reload}
          />
        )}
        {tab === "jobs" && (
          <AdminJobsPanel token={token} onSessionExpired={onSessionExpired} />
        )}
        {tab === "cities" && <AdminCitiesPanel />}
        {tab === "regions" && <AdminRegionPoliciesPanel />}
        {tab === "locations" && <AdminLocationsPanel />}
        {tab === "stats" &&
          (stats.data ? (
            <AdminStatsPanel stats={stats.data} />
          ) : stats.loading ? (
            <p role="status">Loading activity counts…</p>
          ) : null)}
      </div>
    </>
  );
}

export default function AdminPanel({ onClose }: { onClose: () => void }) {
  const [token, setToken] = useState(() =>
    sessionStorage.getItem(ADMIN_SESSION_KEY),
  );
  const [credentials, setCredentials] = useState({ email: "", password: "" });
  const [loading, setLoading] = useState(false);
  const pending = useRef<AbortController | null>(null);
  const addToast = useToastStore((state) => state.addToast);
  useEffect(() => () => pending.current?.abort(), []);
  const onSessionExpired = useCallback(() => {
    setToken(null);
    addToast("Admin session expired. Sign in again.", "error");
  }, [addToast]);
  async function login(event: FormEvent) {
    event.preventDefault();
    if (pending.current) return;
    const controller = new AbortController();
    pending.current = controller;
    setLoading(true);
    try {
      const result = await api.adminLogin(
        credentials.email,
        credentials.password,
        controller.signal,
      );
      if (!controller.signal.aborted) {
        setCredentials({ email: "", password: "" });
        setToken(result.token);
      }
    } catch (failure) {
      if (!controller.signal.aborted)
        addToast(
          failure instanceof Error
            ? failure.message
            : "Sign-in failed. Try again.",
          "error",
        );
    } finally {
      pending.current = null;
      if (!controller.signal.aborted) setLoading(false);
    }
  }
  if (token)
    return (
      <Dashboard
        key={token}
        token={token}
        onSessionExpired={onSessionExpired}
        onClose={onClose}
      />
    );
  return (
    <>
      <Helmet>
        <title>Admin Login | Through Your Letters</title>
      </Helmet>
      <section className="max-w-md mx-auto py-12 space-y-6">
        <h1 className="font-display text-4xl">Admin Portal</h1>
        <form
          onSubmit={(event) => void login(event)}
          className="card p-6 space-y-4"
        >
          <label className="block">
            Email
            <input
              className="block w-full border border-line p-3 mt-1"
              type="email"
              autoComplete="username"
              required
              value={credentials.email}
              disabled={loading}
              onChange={(event) =>
                setCredentials({ ...credentials, email: event.target.value })
              }
            />
          </label>
          <label className="block">
            Password
            <input
              className="block w-full border border-line p-3 mt-1"
              type="password"
              autoComplete="current-password"
              required
              value={credentials.password}
              disabled={loading}
              onChange={(event) =>
                setCredentials({ ...credentials, password: event.target.value })
              }
            />
          </label>
          <button className="btn-dark w-full" type="submit" disabled={loading}>
            {loading ? "Signing in…" : "Sign in"}
          </button>
        </form>
      </section>
    </>
  );
}
