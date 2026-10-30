import { API_BASE_URL } from "../constants";
import { captureUserSession, isCurrentUserSession } from "./userSession";
import { offlineDatabaseName } from "./offlineNamespace";

// Older destination/account-ambiguous records stay preserved without replay.
const DB_NAME = offlineDatabaseName(
  import.meta.env.VITE_OFFLINE_BACKEND || API_BASE_URL,
  import.meta.env.MODE,
  window.location.origin,
);
const STORE_NAME = "uploads";
const OWNER_INDEX = "ownerId";

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
}

function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, 2);
    req.onupgradeneeded = () => {
      const store = req.result.objectStoreNames.contains(STORE_NAME)
        ? req.transaction!.objectStore(STORE_NAME)
        : req.result.createObjectStore(STORE_NAME, {
            keyPath: "id",
            autoIncrement: true,
          });
      if (!store.indexNames.contains(OWNER_INDEX))
        store.createIndex(OWNER_INDEX, OWNER_INDEX);
    };
    req.onsuccess = () => {
      req.result.onversionchange = () => req.result.close();
      resolve(req.result);
    };
    req.onerror = () => reject(req.error);
  });
}

export async function enqueueUpload(data: UploadData): Promise<void> {
  const session = captureUserSession();
  if (!session.user) throw new Error("Sign in before saving an offline upload");
  const db = await openDB();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, "readwrite");
      tx.objectStore(STORE_NAME).add({
        ...data,
        requestKey: data.requestKey ?? crypto.randomUUID(),
        ownerId: session.user!.id,
        createdAt: Date.now(),
      });
      tx.oncomplete = () => resolve();
      tx.onabort = tx.onerror = () => reject(tx.error);
    });
  } finally {
    db.close();
  }
}

export async function getQueuedCount(): Promise<number> {
  const session = captureUserSession();
  if (!session.user) return 0;
  const db = await openDB();
  try {
    return await new Promise<number>((resolve, reject) => {
      const req = db
        .transaction(STORE_NAME)
        .objectStore(STORE_NAME)
        .index(OWNER_INDEX)
        .count(session.user!.id);
      req.onsuccess = () =>
        resolve(isCurrentUserSession(session) ? req.result : 0);
      req.onerror = () => reject(req.error);
    });
  } finally {
    db.close();
  }
}

export async function syncOfflineUploads(): Promise<number> {
  const session = captureUserSession();
  if (!session.token || !session.user) return 0;
  const db = await openDB();
  try {
    const items = await new Promise<QueuedUpload[]>((resolve, reject) => {
      const req = db
        .transaction(STORE_NAME)
        .objectStore(STORE_NAME)
        .index(OWNER_INDEX)
        .getAll(session.user!.id);
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
    let synced = 0;
    for (const item of items) {
      if (!isCurrentUserSession(session)) return 0;
      let requestKey: string | null;
      try {
        requestKey = await getRequestKey(db, item.id, session.user.id);
      } catch {
        break;
      }
      if (!isCurrentUserSession(session)) return 0;
      if (!requestKey) continue;
      const formData = new FormData();
      formData.append("image", item.imageBlob, item.fileName);
      formData.append("contributor_tag", item.contributorTag);
      formData.append("pin_code", item.pinCode);
      formData.append("description", item.description);
      formData.append("city_id", item.cityId);
      try {
        const res = await fetch(`${API_BASE_URL}/api/v1/letterings/upload`, {
          method: "POST",
          headers: {
            Authorization: `Bearer ${session.token}`,
            "Idempotency-Key": requestKey,
          },
          body: formData,
          signal: session.signal,
        });
        if (!isCurrentUserSession(session)) return 0;
        if (!res.ok) break;
        await new Promise<void>((resolve, reject) => {
          const tx = db.transaction(STORE_NAME, "readwrite");
          tx.objectStore(STORE_NAME).delete(item.id);
          tx.oncomplete = () => resolve();
          tx.onabort = tx.onerror = () => reject(tx.error);
        });
        synced++;
      } catch {
        break;
      }
    }
    return isCurrentUserSession(session) ? synced : 0;
  } finally {
    db.close();
  }
}

// Read and write in one transaction so tabs assign the same key to older records.
function getRequestKey(
  db: IDBDatabase,
  id: number,
  ownerId: string,
): Promise<string | null> {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const store = tx.objectStore(STORE_NAME);
    let key: string | null = null;
    const request = store.get(id);
    request.onsuccess = () => {
      const record = request.result as QueuedUpload | undefined;
      if (!record || record.ownerId !== ownerId) return;
      key = record.requestKey ?? crypto.randomUUID();
      if (!record.requestKey) store.put({ ...record, requestKey: key });
    };
    tx.oncomplete = () => resolve(key);
    tx.onabort = tx.onerror = () => reject(tx.error);
  });
}
