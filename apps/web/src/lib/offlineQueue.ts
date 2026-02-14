import { API_BASE_URL } from "../constants";
import { USER_SESSION_KEY } from "./api";

const DB_NAME = "tyl-offline";
const DB_VERSION = 1;
const STORE_NAME = "uploads";

interface QueuedUpload {
  id: number;
  imageBlob: Blob;
  fileName: string;
  contributorTag: string;
  pinCode: string;
  description: string;
  cityId: string;
  createdAt: number;
}

function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME, {
          keyPath: "id",
          autoIncrement: true,
        });
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export async function enqueueUpload(data: {
  imageBlob: Blob;
  fileName: string;
  contributorTag: string;
  pinCode: string;
  description: string;
  cityId: string;
}): Promise<void> {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    tx.objectStore(STORE_NAME).add({
      ...data,
      createdAt: Date.now(),
    });
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

export async function getQueuedCount(): Promise<number> {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readonly");
    const req = tx.objectStore(STORE_NAME).count();
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export async function syncOfflineUploads(): Promise<number> {
  const db = await openDB();

  const items: QueuedUpload[] = await new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readonly");
    const req = tx.objectStore(STORE_NAME).getAll();
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });

  if (items.length === 0) return 0;

  let synced = 0;

  for (const item of items) {
    const formData = new FormData();
    formData.append("image", item.imageBlob, item.fileName);
    formData.append("contributor_tag", item.contributorTag);
    formData.append("pin_code", item.pinCode);
    formData.append("description", item.description);
    formData.append("city_id", item.cityId);

    try {
      const token = sessionStorage.getItem(USER_SESSION_KEY);
      const res = await fetch(`${API_BASE_URL}/api/v1/letterings/upload`, {
        method: "POST",
        headers: token ? { Authorization: `Bearer ${token}` } : undefined,
        body: formData,
      });

      if (res.ok) {
        await new Promise<void>((resolve, reject) => {
          const tx = db.transaction(STORE_NAME, "readwrite");
          tx.objectStore(STORE_NAME).delete(item.id);
          tx.oncomplete = () => resolve();
          tx.onerror = () => reject(tx.error);
        });
        synced++;
      }
    } catch {
      // Network still down or server error - stop trying.
      break;
    }
  }

  return synced;
}
