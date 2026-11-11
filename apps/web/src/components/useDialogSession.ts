import { useCallback, useEffect, useRef, useState } from "react";
import { captureUserSession, isCurrentUserSession } from "../lib/userSession";

export function useDialogSession(onClose: () => void) {
  const [session] = useState(captureUserSession);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    if (session.signal.aborted || !session.token) onClose();
    session.signal.addEventListener("abort", onClose, { once: true });
    return () => session.signal.removeEventListener("abort", onClose);
  }, [session, onClose]);
  const current = useCallback(
    () => mounted.current && isCurrentUserSession(session),
    [session],
  );
  return { session, current };
}
