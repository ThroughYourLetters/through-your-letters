import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  discardSavedUpload,
  getSavedPhoto,
  getSavedUploads,
  retrySavedUpload,
  subscribeSavedUploads,
  type SavedUpload,
} from "../lib/offlineQueue";
import { captureUserSession, isCurrentUserSession } from "../lib/userSession";
import { useAuthStore } from "../store/useAuthStore";
import { useToastStore } from "../store/useToastStore";

function guidance(item: SavedUpload): string {
  switch (item.issue) {
    case "session":
      return "Sign in again to upload. Your photo is kept on this device.";
    case "rate_limit":
      return item.retryAfter
        ? `Uploads are rate limited. Retry after ${new Date(item.retryAfter).toLocaleString()}.`
        : "Uploads are rate limited. Wait before retrying.";
    case "invalid":
      return "This submission was not accepted. Download the photo, check its details, and submit it again.";
    case "too_large":
      return "This photo is too large. Download it and submit a smaller copy.";
    case "network":
      return "The connection was interrupted. Your photo is kept; retry when you are online.";
    case "server":
      return "The upload service is unavailable. Your photo is kept; try again later.";
    default:
      return "Saved on this device. It will upload when you are signed in and online.";
  }
}

export default function SavedUploadsPage() {
  const { user, revision } = useAuthStore();
  const addToast = useToastStore((state) => state.addToast);
  const queryClient = useQueryClient();
  const [cursors, setCursors] = useState<number[]>([0]);
  const [busy, setBusy] = useState<number | null>(null);
  const [error, setError] = useState("");
  const after = cursors.at(-1) ?? 0;
  const query = useQuery({
    queryKey: ["saved-uploads", user?.id, revision, after],
    queryFn: () => getSavedUploads(after),
    enabled: !!user,
    retry: false,
  });

  useEffect(
    () =>
      subscribeSavedUploads(() => {
        void queryClient.invalidateQueries({ queryKey: ["saved-uploads"] });
      }),
    [queryClient],
  );

  useEffect(() => {
    if (
      !query.data?.items.some(
        (item) =>
          item.state === "retrying" || (item.retryAfter ?? 0) > Date.now(),
      )
    )
      return;
    const timer = window.setInterval(() => {
      void queryClient.invalidateQueries({ queryKey: ["saved-uploads"] });
    }, 2000);
    return () => window.clearInterval(timer);
  }, [query.data, queryClient]);

  async function act(id: number, action: "retry" | "discard" | "download") {
    const session = captureUserSession();
    if (
      action === "discard" &&
      !window.confirm("Discard this saved photo? This deletes its local copy.")
    )
      return;
    setBusy(id);
    setError("");
    try {
      if (action === "retry") {
        const count = await retrySavedUpload(id);
        if (count > 0 && isCurrentUserSession(session))
          addToast("Saved photos uploaded", "success");
      } else if (action === "discard") await discardSavedUpload(id);
      else {
        const photo = await getSavedPhoto(id);
        if (photo && isCurrentUserSession(session)) {
          const url = URL.createObjectURL(photo.blob);
          const link = document.createElement("a");
          link.href = url;
          link.download = photo.fileName;
          link.click();
          window.setTimeout(() => URL.revokeObjectURL(url), 1000);
        }
      }
      if (isCurrentUserSession(session)) await query.refetch();
    } catch (failure) {
      if (isCurrentUserSession(session))
        setError(
          failure instanceof Error
            ? failure.message
            : "Saved upload could not be changed",
        );
    } finally {
      if (isCurrentUserSession(session)) setBusy(null);
    }
  }

  if (!user)
    return (
      <section className="max-w-xl mx-auto space-y-5 py-12">
        <h1 className="font-display text-4xl">Saved uploads</h1>
        <p>
          Sign in to view your saved photos. They stay on this device and are
          available only to the account that saved them.
        </p>
        <Link className="btn-dark" to="/auth">
          Sign in
        </Link>
      </section>
    );

  return (
    <section className="max-w-3xl mx-auto space-y-6 pb-12">
      <h1 className="font-display text-4xl">Saved uploads</h1>
      <p>
        These photos are saved on this device until they upload or you discard
        them. Clearing browser storage removes them.
      </p>
      <div className="flex flex-wrap gap-4">
        <Link className="btn-outline" to="/contribute">
          Add photos
        </Link>
        <Link className="btn-outline" to="/me/uploads">
          My uploads
        </Link>
      </div>
      {(error || query.isError) && (
        <p role="alert" className="text-red-700">
          {error ||
            "Saved photos could not be loaded. Check that browser storage is available."}
        </p>
      )}
      {query.isError && (
        <button className="btn-outline" onClick={() => void query.refetch()}>
          Try loading again
        </button>
      )}
      {query.isPending ? (
        <p role="status">Loading saved photos…</p>
      ) : query.data?.items.length === 0 ? (
        <p>No saved photos on this page.</p>
      ) : (
        <ul className="space-y-4">
          {query.data?.items.map((item) => (
            <li key={item.id} className="card p-5 space-y-3">
              <h2 className="font-semibold break-all">{item.fileName}</h2>
              <p className="text-sm text-muted">
                {Math.ceil(item.bytes / 1024).toLocaleString()} KB · Saved{" "}
                {new Date(item.createdAt).toLocaleString()}
              </p>
              <p className="font-medium" role="status">
                {item.state === "retrying"
                  ? "Uploading"
                  : item.state === "queued"
                    ? "Queued"
                    : item.state === "blocked"
                      ? "Blocked"
                      : "Failed"}
              </p>
              <p>
                {item.state === "retrying"
                  ? "This photo is uploading. Keep this tab open until it finishes."
                  : guidance(item)}
              </p>
              <div className="flex flex-wrap gap-3">
                <button
                  className="btn-dark"
                  disabled={
                    busy !== null ||
                    item.state === "retrying" ||
                    (item.retryAfter ?? 0) > Date.now()
                  }
                  onClick={() => void act(item.id, "retry")}
                >
                  Retry upload
                </button>
                <button
                  className="btn-outline"
                  disabled={busy !== null}
                  onClick={() => void act(item.id, "download")}
                >
                  Download photo
                </button>
                <button
                  className="btn-outline"
                  disabled={busy !== null || item.state === "retrying"}
                  onClick={() => void act(item.id, "discard")}
                >
                  Discard
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
      <nav aria-label="Saved upload pages" className="flex gap-3">
        <button
          className="btn-outline"
          disabled={cursors.length === 1 || query.isFetching || busy !== null}
          onClick={() => setCursors((previous) => previous.slice(0, -1))}
        >
          Previous page
        </button>
        <button
          className="btn-outline"
          disabled={!query.data?.next || query.isFetching || busy !== null}
          onClick={() => {
            const next = query.data?.next;
            if (next) setCursors((previous) => [...previous, next]);
          }}
        >
          Next page
        </button>
      </nav>
    </section>
  );
}
