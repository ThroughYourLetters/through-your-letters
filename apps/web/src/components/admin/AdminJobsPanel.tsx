import { useEffect, useRef, useState } from "react";
import type {
  AnalysisJobItem,
  AnalysisJobsResponse,
  CleanupJobItem,
  CleanupJobsResponse,
} from "@ttl/types";
import { ADMIN_SESSION_KEY, ApiError, api } from "../../lib/api";

type Jobs =
  | { kind: "analysis"; page: AnalysisJobsResponse }
  | { kind: "cleanup"; page: CleanupJobsResponse };

const failureLabels = {
  processing_failed: "Image analysis failed",
  timed_out: "Image analysis timed out",
  lease_expired: "A worker stopped responding",
  interrupted: "Processing was interrupted",
  storage_unavailable: "File storage is unavailable",
  unrecognized_url: "The file address needs review",
};

export default function AdminJobsPanel({
  token,
  onSessionExpired,
}: {
  token: string;
  onSessionExpired: () => void;
}) {
  const [kind, setKind] = useState<Jobs["kind"]>("analysis");
  const [after, setAfter] = useState<string>();
  const [refresh, setRefresh] = useState(0);
  const [jobs, setJobs] = useState<Jobs | null>(null);
  const [loading, setLoading] = useState(false);
  const [pendingId, setPendingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const requests = useRef(new Set<AbortController>());

  useEffect(() => {
    const active = requests.current;
    return () => {
      for (const controller of active) controller.abort();
      active.clear();
    };
  }, [token]);

  useEffect(() => {
    const controller = new AbortController();
    const active = requests.current;
    active.add(controller);
    setLoading(true);
    setJobs(null);
    setError(null);
    void (async () => {
      try {
        const result: Jobs =
          kind === "analysis"
            ? {
                kind,
                page: await api.adminAnalysisJobs(after, controller.signal),
              }
            : {
                kind,
                page: await api.adminCleanupJobs(after, controller.signal),
              };
        if (
          !controller.signal.aborted &&
          sessionStorage.getItem(ADMIN_SESSION_KEY) === token
        )
          setJobs(result);
      } catch (failure) {
        if (controller.signal.aborted) return;
        if (
          failure instanceof ApiError &&
          failure.status === 401 &&
          sessionStorage.getItem(ADMIN_SESSION_KEY) === null
        ) {
          onSessionExpired();
        } else if (sessionStorage.getItem(ADMIN_SESSION_KEY) === token) {
          setError(
            failure instanceof Error
              ? failure.message
              : "Could not load failed jobs",
          );
        }
      } finally {
        active.delete(controller);
        if (
          !controller.signal.aborted &&
          sessionStorage.getItem(ADMIN_SESSION_KEY) === token
        )
          setLoading(false);
      }
    })();
    return () => {
      controller.abort();
      active.delete(controller);
    };
  }, [kind, after, refresh, token, onSessionExpired]);

  async function retry(
    job:
      | { kind: "analysis"; item: AnalysisJobItem }
      | { kind: "cleanup"; item: CleanupJobItem },
  ) {
    const controller = new AbortController();
    requests.current.add(controller);
    setPendingId(job.item.id);
    setError(null);
    setNotice(null);
    try {
      if (job.kind === "analysis") {
        await api.adminRetryAnalysis(
          job.item.id,
          { finished_at: job.item.finished_at },
          controller.signal,
        );
      } else {
        await api.adminRetryCleanup(
          job.item.id,
          {
            available_at: job.item.available_at,
            attempts: job.item.attempts,
          },
          controller.signal,
        );
      }
      if (
        !controller.signal.aborted &&
        sessionStorage.getItem(ADMIN_SESSION_KEY) === token
      ) {
        setNotice("Retry queued.");
        setRefresh((value) => value + 1);
      }
    } catch (failure) {
      if (controller.signal.aborted) return;
      if (
        failure instanceof ApiError &&
        failure.status === 401 &&
        sessionStorage.getItem(ADMIN_SESSION_KEY) === null
      ) {
        onSessionExpired();
      } else if (sessionStorage.getItem(ADMIN_SESSION_KEY) === token) {
        setError(
          failure instanceof Error
            ? failure.message
            : "Could not queue the retry",
        );
      }
    } finally {
      requests.current.delete(controller);
      if (
        !controller.signal.aborted &&
        sessionStorage.getItem(ADMIN_SESSION_KEY) === token
      )
        setPendingId(null);
    }
  }

  const busy = loading || pendingId !== null;
  return (
    <section className="card p-6 space-y-5" aria-labelledby="admin-jobs-title">
      <h2 id="admin-jobs-title" className="font-display text-2xl font-semibold">
        Failed jobs
      </h2>
      <p className="text-sm text-muted">
        Review the cause before retrying. Image analysis gets a fresh attempt
        budget only when an administrator requests it.
      </p>
      <div className="flex flex-wrap items-end gap-3">
        <label className="flex flex-col gap-1">
          Job type
          <select
            className="rounded-lg border border-line bg-surface p-2"
            value={kind}
            disabled={busy}
            onChange={(event) => {
              setKind(event.target.value as Jobs["kind"]);
              setAfter(undefined);
              setNotice(null);
            }}
          >
            <option value="analysis">Image analysis</option>
            <option value="cleanup">File cleanup</option>
          </select>
        </label>
        <button
          className="btn-secondary"
          disabled={busy}
          onClick={() => setRefresh((value) => value + 1)}
        >
          Refresh jobs
        </button>
      </div>
      {loading && <p role="status">Loading jobs...</p>}
      {notice && <p role="status">{notice}</p>}
      {error && <p role="alert">{error}</p>}
      {jobs?.page.items.length === 0 && <p>No failed jobs on this page.</p>}
      {jobs?.kind === "analysis" &&
        jobs.page.items.map((item) => (
          <article
            key={item.id}
            className="rounded-xl border border-line p-4 space-y-2 break-words"
          >
            <h3 className="font-semibold">{item.contributor_tag}</h3>
            <p className="text-sm text-muted">Upload {item.id}</p>
            {item.thumbnail_small && (
              <img
                src={item.thumbnail_small}
                alt="Upload whose image analysis failed"
                className="h-28 max-w-full rounded-lg object-contain"
              />
            )}
            <p>
              {item.last_error_code
                ? failureLabels[item.last_error_code]
                : "No failure reason recorded"}
            </p>
            <p>
              {item.attempts} of {item.max_attempts} attempts ·{" "}
              {new Date(item.finished_at).toLocaleString()}
            </p>
            {!item.can_retry && (
              <p>This upload has been reviewed and cannot be retried.</p>
            )}
            <button
              className="btn-secondary"
              disabled={busy || !item.can_retry}
              onClick={() => void retry({ kind: "analysis", item })}
            >
              {pendingId === item.id ? "Queuing retry..." : "Retry analysis"}
            </button>
          </article>
        ))}
      {jobs?.kind === "cleanup" &&
        jobs.page.items.map((item) => (
          <article
            key={item.id}
            className="rounded-xl border border-line p-4 space-y-2 break-words"
          >
            <h3 className="font-semibold">File cleanup {item.id}</h3>
            <p>
              {item.last_error_code
                ? failureLabels[item.last_error_code]
                : "No failure reason recorded"}
            </p>
            <p>{item.attempts} deletion attempts</p>
            <p>
              {item.state === "ready"
                ? `Automatic retry scheduled for ${new Date(item.available_at).toLocaleString()}`
                : "Review storage settings before retrying this file."}
            </p>
            <button
              className="btn-secondary"
              disabled={busy}
              onClick={() => void retry({ kind: "cleanup", item })}
            >
              {pendingId === item.id ? "Queuing retry..." : "Retry cleanup now"}
            </button>
          </article>
        ))}
      <div className="flex flex-wrap gap-3">
        {after && (
          <button
            className="btn-secondary"
            disabled={busy}
            onClick={() => setAfter(undefined)}
          >
            First page
          </button>
        )}
        {jobs?.page.next_cursor && (
          <button
            className="btn-secondary"
            disabled={busy}
            onClick={() => setAfter(jobs.page.next_cursor!)}
          >
            Next page
          </button>
        )}
      </div>
    </section>
  );
}
