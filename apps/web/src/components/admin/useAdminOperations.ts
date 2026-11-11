import { useCallback, useEffect, useRef, useState } from "react";
import { ADMIN_SESSION_KEY, ApiError } from "../../lib/api";
import { useToastStore } from "../../store/useToastStore";

export interface AdminSessionProps {
  token: string;
  onSessionExpired: () => void;
}

function describeFailure(
  error: unknown,
  session: AdminSessionProps,
): string | null {
  if (error instanceof DOMException && error.name === "AbortError") return null;
  const current = sessionStorage.getItem(ADMIN_SESSION_KEY);
  if (
    error instanceof ApiError &&
    error.status === 401 &&
    (current === null || current === session.token)
  ) {
    session.onSessionExpired();
    return null;
  }
  if (current !== session.token) return null;
  return error instanceof Error
    ? error.message
    : "The request failed. Please try again.";
}

export function useAdminResource<T>(
  session: AdminSessionProps,
  load: (signal: AbortSignal) => Promise<T>,
) {
  const { token, onSessionExpired } = session;
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [revision, setRevision] = useState(0);
  const reload = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    const controller = new AbortController();
    setLoading(true);
    setData(null);
    setError(null);
    void load(controller.signal)
      .then((result) => {
        if (
          !controller.signal.aborted &&
          sessionStorage.getItem(ADMIN_SESSION_KEY) === token
        )
          setData(result);
      })
      .catch((failure) => {
        if (!controller.signal.aborted)
          setError(describeFailure(failure, { token, onSessionExpired }));
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [token, onSessionExpired, load, revision]);
  return { data, error, loading, reload };
}

type ActionResult<T> = { ok: true; value: T } | { ok: false };

export function useAdminAction(session: AdminSessionProps) {
  const { token, onSessionExpired } = session;
  const addToast = useToastStore((state) => state.addToast);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const current = useCallback(
    () =>
      mounted.current && sessionStorage.getItem(ADMIN_SESSION_KEY) === token,
    [token],
  );
  const run = useCallback(
    async <T>(work: () => Promise<T>): Promise<ActionResult<T>> => {
      if (pending.current || !current()) return { ok: false };
      pending.current = true;
      setBusy(true);
      try {
        const value = await work();
        return current() ? { ok: true, value } : { ok: false };
      } catch (failure) {
        if (mounted.current) {
          const message = describeFailure(failure, { token, onSessionExpired });
          if (message) addToast(message, "error");
        }
        return { ok: false };
      } finally {
        pending.current = false;
        if (mounted.current) setBusy(false);
      }
    },
    [token, onSessionExpired, addToast, current],
  );
  return { busy, run, current };
}
