import { useEffect, useRef } from "react";
import { useRegisterSW } from "virtual:pwa-register/react";

export function useAppUpdate() {
  const reloadRequested = useRef(false);
  const {
    needRefresh: [updateReady],
    updateServiceWorker,
  } = useRegisterSW({
    onNeedReload() {
      // Activation affects every tab; only a tab that requested reload should refresh.
    },
  });

  useEffect(() => {
    const reload = () => {
      if (reloadRequested.current) window.location.reload();
    };
    navigator.serviceWorker?.addEventListener("controllerchange", reload);
    return () =>
      navigator.serviceWorker?.removeEventListener("controllerchange", reload);
  }, []);

  async function reloadPage() {
    reloadRequested.current = true;
    try {
      const registration = await navigator.serviceWorker?.getRegistration();
      if (registration?.waiting) await updateServiceWorker(true);
      else window.location.reload();
    } catch {
      window.location.reload();
    }
  }

  return { updateReady, reloadPage };
}
