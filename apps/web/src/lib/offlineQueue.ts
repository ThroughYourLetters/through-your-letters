import { API_BASE_URL } from "../constants";
import {
  captureUserSession,
  expireUserSession,
  isCurrentUserSession,
  sessionChanged,
  type UserSession,
} from "./userSession";
import { offlineDatabaseName } from "./offlineNamespace";
import { webLimits, uploadTimeoutMs } from "./runtimeConfig";

// Records without a verified owner or destination remain preserved without replay.
const DB_NAME = offlineDatabaseName(
  import.meta.env.VITE_OFFLINE_BACKEND || API_BASE_URL,
  import.meta.env.MODE,
  window.location.origin,
);
const STORE = "uploads";
const OWNER = "ownerId";
const OWNER_ID = "ownerId_id";
const CHANGED = "ttl-saved-uploads-changed";
const channel =
  typeof BroadcastChannel === "undefined"
    ? null
    : new BroadcastChannel(`ttl-saved-uploads:${DB_NAME}`);
channel?.addEventListener("message", () =>
  window.dispatchEvent(new Event(CHANGED)),
);
const activeReplays = new Map<string, Promise<number>>();

type Issue =
  "session" | "rate_limit" | "invalid" | "too_large" | "network" | "server";
type StoredState = "queued" | "blocked" | "failed";
interface UploadData {
  requestKey?: string;
  imageBlob: Blob;
  fileName: string;
  contributorTag: string;
  pinCode: string;
  description: string;
  cityId: string;
}
interface QueuedUpload extends UploadData {
  id: number;
  ownerId: string;
  createdAt: number;
  state?: StoredState;
  issue?: Issue;
  retryAfter?: number;
  attempts?: number;
  lease?: { token: string; until: number };
}
export interface SavedUpload {
  id: number;
  fileName: string;
  createdAt: number;
  bytes: number;
  state: StoredState | "retrying";
  issue?: Issue;
  retryAfter?: number;
  attempts: number;
}

function changed() {
  window.dispatchEvent(new Event(CHANGED));
  channel?.postMessage(null);
}
export function subscribeSavedUploads(listener: () => void): () => void {
  window.addEventListener(CHANGED, listener);
  return () => window.removeEventListener(CHANGED, listener);
}
function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, 3);
    request.onupgradeneeded = () => {
      const store = request.result.objectStoreNames.contains(STORE)
        ? request.transaction!.objectStore(STORE)
        : request.result.createObjectStore(STORE, {
            keyPath: "id",
            autoIncrement: true,
          });
      if (!store.indexNames.contains(OWNER)) store.createIndex(OWNER, OWNER);
      if (!store.indexNames.contains(OWNER_ID))
        store.createIndex(OWNER_ID, [OWNER, "id"]);
    };
    request.onsuccess = () => {
      request.result.onversionchange = () => request.result.close();
      resolve(request.result);
    };
    request.onerror = () => reject(request.error);
  });
}

async function inStore<T>(
  session: UserSession,
  mode: IDBTransactionMode,
  work: (
    store: IDBObjectStore,
    done: (value: T) => void,
    fail: (error: Error) => void,
  ) => void,
  releaseOldClaim = false,
): Promise<T> {
  const db = await openDB();
  try {
    if (!session.user || (!releaseOldClaim && !isCurrentUserSession(session)))
      throw sessionChanged();
    return await new Promise<T>((resolve, reject) => {
      const tx = db.transaction(STORE, mode);
      let value!: T;
      let failure: Error | null = null;
      const abort = () => {
        try {
          tx.abort();
        } catch {
          /* Already complete. */
        }
      };
      if (!releaseOldClaim)
        session.signal.addEventListener("abort", abort, { once: true });
      const cleanup = () => session.signal.removeEventListener("abort", abort);
      tx.oncomplete = () => {
        cleanup();
        if (!releaseOldClaim && !isCurrentUserSession(session))
          reject(sessionChanged());
        else resolve(value);
      };
      tx.onabort = () => {
        cleanup();
        reject(failure ?? tx.error ?? sessionChanged());
      };
      try {
        work(
          tx.objectStore(STORE),
          (result) => {
            value = result;
          },
          (error) => {
            failure = error;
            abort();
          },
        );
      } catch (error) {
        failure =
          error instanceof Error
            ? error
            : new Error("Saved uploads could not be read");
        abort();
      }
    });
  } finally {
    db.close();
  }
}

export async function enqueueUpload(data: UploadData): Promise<void> {
  const session = captureUserSession();
  if (!session.user) throw new Error("Sign in before saving an upload");
  await inStore<void>(session, "readwrite", (store, done, fail) => {
    const request = store.index(OWNER).getAll(session.user!.id);
    request.onsuccess = () => {
      const rows = request.result as QueuedUpload[];
      if (
        data.requestKey &&
        rows.some((row) => row.requestKey === data.requestKey)
      ) {
        done();
        return;
      }
      const bytes = rows.reduce((total, row) => total + row.imageBlob.size, 0);
      if (
        rows.length >= webLimits.offline_max_entries ||
        data.imageBlob.size > webLimits.offline_max_bytes - bytes
      ) {
        fail(
          new Error(
            "Saved uploads are full. Upload or discard an existing photo before saving another.",
          ),
        );
        return;
      }
      store.add({
        ...data,
        requestKey: data.requestKey ?? crypto.randomUUID(),
        ownerId: session.user!.id,
        createdAt: Date.now(),
        state: "queued",
        attempts: 0,
      });
      done();
    };
  });
  changed();
}

export async function getQueuedCount(): Promise<number> {
  const session = captureUserSession();
  if (!session.user) return 0;
  try {
    return await inStore<number>(session, "readonly", (store, done) => {
      const request = store.index(OWNER).count(session.user!.id);
      request.onsuccess = () => done(request.result);
    });
  } catch (error) {
    if (!isCurrentUserSession(session)) return 0;
    throw error;
  }
}

export async function getSavedUploads(
  after = 0,
): Promise<{ items: SavedUpload[]; next: number | null }> {
  const session = captureUserSession();
  if (!session.user) return { items: [], next: null };
  return inStore(session, "readonly", (store, done) => {
    const items: SavedUpload[] = [];
    const range = IDBKeyRange.bound(
      [session.user!.id, after],
      [session.user!.id, Infinity],
      true,
    );
    const request = store.index(OWNER_ID).openCursor(range);
    request.onsuccess = () => {
      const cursor = request.result;
      if (!cursor) {
        done({ items, next: null });
        return;
      }
      if (items.length === webLimits.offline_page_size) {
        done({ items, next: items.at(-1)!.id });
        return;
      }
      const row = cursor.value as QueuedUpload;
      items.push({
        id: row.id,
        fileName: row.fileName,
        createdAt: row.createdAt,
        bytes: row.imageBlob.size,
        state:
          row.lease && row.lease.until > Date.now()
            ? "retrying"
            : (row.state ?? "queued"),
        issue: row.issue,
        retryAfter: row.retryAfter,
        attempts: row.attempts ?? 0,
      });
      cursor.continue();
    };
  });
}

export async function getSavedPhoto(
  id: number,
): Promise<{ blob: Blob; fileName: string } | null> {
  const session = captureUserSession();
  if (!session.user) return null;
  return inStore(session, "readonly", (store, done) => {
    const request = store.get(id);
    request.onsuccess = () => {
      const row = request.result as QueuedUpload | undefined;
      done(
        row?.ownerId === session.user!.id
          ? { blob: row.imageBlob, fileName: row.fileName }
          : null,
      );
    };
  });
}

async function editSaved(id: number, retry: boolean): Promise<UserSession> {
  const session = captureUserSession();
  if (!session.user) throw new Error("Sign in to manage saved uploads");
  await inStore<void>(session, "readwrite", (store, done, fail) => {
    const request = store.get(id);
    request.onsuccess = () => {
      const row = request.result as QueuedUpload | undefined;
      if (!row || row.ownerId !== session.user!.id) {
        done();
        return;
      }
      if (row.lease && row.lease.until > Date.now()) {
        fail(
          new Error(
            "This photo is uploading. Wait for it to finish before changing it.",
          ),
        );
        return;
      }
      if (!retry) store.delete(id);
      else {
        if (row.retryAfter && row.retryAfter > Date.now()) {
          fail(
            new Error(
              "Uploads are rate limited. Wait until the retry time shown.",
            ),
          );
          return;
        }
        store.put({
          ...row,
          state: "queued",
          issue: undefined,
          retryAfter: undefined,
          lease: undefined,
        });
      }
      done();
    };
  });
  changed();
  return session;
}
export async function discardSavedUpload(id: number): Promise<void> {
  await editSaved(id, false);
}
export async function retrySavedUpload(id: number): Promise<number> {
  const session = captureUserSession();
  const active = activeReplays.get(
    `ttl-offline-replay:${DB_NAME}:${session.user?.id}:${session.revision}`,
  );
  if (active) await active;
  if (!isCurrentUserSession(session)) throw sessionChanged();
  await editSaved(id, true);
  return syncOfflineUploads();
}

export function syncOfflineUploads(): Promise<number> {
  const session = captureUserSession();
  if (!session.token || !session.user || !navigator.onLine)
    return Promise.resolve(0);
  const lockName = `ttl-offline-replay:${DB_NAME}:${session.user.id}`;
  const localKey = `${lockName}:${session.revision}`;
  const existing = activeReplays.get(localKey);
  if (existing) return existing.then(() => 0);
  const work = Promise.resolve()
    .then(async () =>
      navigator.locks
        ? await navigator.locks.request(
            lockName,
            { signal: session.signal },
            () => replayUploads(session, true),
          )
        : await replayUploads(session, false),
    )
    .catch(() => 0)
    .finally(() => activeReplays.delete(localKey));
  activeReplays.set(localKey, work);
  return work;
}

async function claimUpload(
  session: UserSession,
  id: number,
  exclusive: boolean,
): Promise<QueuedUpload | null> {
  return inStore(session, "readwrite", (store, done) => {
    const request = store.get(id);
    request.onsuccess = () => {
      const row = request.result as QueuedUpload | undefined;
      if (
        !row ||
        row.ownerId !== session.user!.id ||
        row.state === "failed" ||
        (row.retryAfter ?? 0) > Date.now() ||
        (!exclusive && row.lease && row.lease.until > Date.now())
      ) {
        done(null);
        return;
      }
      const claimed: QueuedUpload = {
        ...row,
        requestKey: row.requestKey ?? crypto.randomUUID(),
        attempts: (row.attempts ?? 0) + 1,
        lease: {
          token: crypto.randomUUID(),
          until:
            Date.now() + uploadTimeoutMs + webLimits.offline_lease_margin_ms,
        },
      };
      store.put(claimed);
      done(claimed);
    };
  });
}

async function finishClaim(
  session: UserSession,
  row: QueuedUpload,
  outcome:
    { state: StoredState; issue?: Issue; retryAfter?: number } | "uploaded",
  releaseOldClaim = false,
): Promise<boolean> {
  const updated = await inStore<boolean>(
    session,
    "readwrite",
    (store, done) => {
      const request = store.get(row.id);
      request.onsuccess = () => {
        const current = request.result as QueuedUpload | undefined;
        if (
          !current ||
          current.ownerId !== session.user!.id ||
          current.lease?.token !== row.lease?.token
        ) {
          done(false);
          return;
        }
        if (outcome === "uploaded") store.delete(row.id);
        else store.put({ ...current, ...outcome, lease: undefined });
        done(true);
      };
    },
    releaseOldClaim,
  );
  if (updated) changed();
  return updated;
}

function retryTime(response: Response): number {
  const header = response.headers.get("Retry-After")?.trim();
  const delay = header && /^\d+$/.test(header) ? Number(header) * 1000 : NaN;
  const parsed = header && !Number.isFinite(delay) ? Date.parse(header) : NaN;
  if (
    Number.isFinite(delay) &&
    delay >= 0 &&
    delay <= Number.MAX_SAFE_INTEGER - Date.now()
  )
    return Date.now() + delay;
  if (Number.isFinite(parsed)) return Math.max(Date.now(), parsed);
  return Date.now() + webLimits.offline_retry_delay_seconds * 1000;
}

async function replayUploads(
  session: UserSession,
  exclusive: boolean,
): Promise<number> {
  if (!isCurrentUserSession(session)) return 0;
  const ids = await inStore<IDBValidKey[]>(
    session,
    "readonly",
    (store, done) => {
      const ids: IDBValidKey[] = [];
      const request = store
        .index(OWNER)
        .openCursor(IDBKeyRange.only(session.user!.id));
      request.onsuccess = () => {
        const cursor = request.result;
        if (!cursor) {
          done(ids);
          return;
        }
        const row = cursor.value as QueuedUpload;
        if (row.issue === "rate_limit" && (row.retryAfter ?? 0) > Date.now()) {
          done([]);
          return;
        }
        ids.push(cursor.primaryKey);
        cursor.continue();
      };
    },
  );
  let synced = 0;
  for (const id of ids) {
    if (!isCurrentUserSession(session)) return 0;
    const row = await claimUpload(session, Number(id), exclusive);
    if (!row) continue;
    changed();
    const form = new FormData();
    form.append("image", row.imageBlob, row.fileName);
    form.append("contributor_tag", row.contributorTag);
    form.append("pin_code", row.pinCode);
    form.append("description", row.description);
    form.append("city_id", row.cityId);
    const controller = new AbortController();
    const abort = () => controller.abort();
    session.signal.addEventListener("abort", abort, { once: true });
    const timeout = window.setTimeout(abort, uploadTimeoutMs);
    try {
      const response = await fetch(`${API_BASE_URL}/api/v1/letterings/upload`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${session.token}`,
          "Idempotency-Key": row.requestKey!,
        },
        body: form,
        signal: controller.signal,
      });
      if (!isCurrentUserSession(session)) throw sessionChanged();
      if (response.ok) {
        if (await finishClaim(session, row, "uploaded")) synced++;
      } else if (response.status === 401) {
        await finishClaim(session, row, { state: "blocked", issue: "session" });
        expireUserSession(session);
        break;
      } else if (response.status === 429) {
        await finishClaim(session, row, {
          state: "blocked",
          issue: "rate_limit",
          retryAfter: retryTime(response),
        });
        break;
      } else if (response.status >= 500) {
        await finishClaim(session, row, { state: "blocked", issue: "server" });
        break;
      } else
        await finishClaim(session, row, {
          state: "failed",
          issue: response.status === 413 ? "too_large" : "invalid",
        });
    } catch {
      // If release fails, the fenced claim remains recoverable through lease expiry.
      await finishClaim(
        session,
        row,
        { state: "blocked", issue: "network" },
        !isCurrentUserSession(session),
      ).catch(() => {});
      break;
    } finally {
      window.clearTimeout(timeout);
      session.signal.removeEventListener("abort", abort);
    }
  }
  return isCurrentUserSession(session) ? synced : 0;
}
