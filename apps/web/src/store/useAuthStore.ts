import { create } from "zustand";
import { api, type AuthUser, registerUnauthorizedHandler } from "../lib/api";
import {
  captureUserSession,
  isCurrentUserSession,
  replaceUserSession,
  sessionChanged,
} from "../lib/userSession";
import { useToastStore } from "./useToastStore";

interface AuthState {
  user: AuthUser | null;
  token: string | null;
  revision: number;
  loading: boolean;
  hydrated: boolean;
  hydrate: () => Promise<void>;
  login: (email: string, password: string) => Promise<void>;
  register: (payload: {
    email: string;
    password: string;
    display_name?: string;
  }) => Promise<void>;
  logout: () => void;
  refreshMe: () => Promise<void>;
}

export const useAuthStore = create<AuthState>((set) => {
  let operation = 0;
  const initial = captureUserSession();
  const commit = (token: string | null, user: AuthUser | null) => {
    const previous = captureUserSession();
    const revision = replaceUserSession(token, user);
    if (revision !== previous.revision) useToastStore.setState({ toasts: [] });
    set({ user, token, revision, hydrated: true, loading: false });
  };
  const logout = () => {
    operation++;
    commit(null, null);
  };
  registerUnauthorizedHandler(logout);
  const refresh = async () => {
    const current = ++operation;
    const session = captureUserSession();
    if (!session.token) {
      commit(null, null);
      return;
    }
    set({ loading: true });
    try {
      const user = await api.getCurrentUser();
      if (current === operation && isCurrentUserSession(session))
        commit(session.token, user);
    } catch {
      // A network failure must not discard the saved account or its offline work.
      // A current 401 invokes logout through the API client's session guard.
      if (current === operation && isCurrentUserSession(session))
        set({ loading: false, hydrated: true });
    }
  };
  const authenticate = async (
    request: () => Promise<{ token: string; user: AuthUser }>,
  ) => {
    const current = ++operation;
    if (captureUserSession().token) commit(null, null);
    set({ loading: true });
    try {
      const result = await request();
      if (current !== operation) throw sessionChanged();
      commit(result.token, result.user);
    } catch (error) {
      if (current === operation) set({ loading: false });
      throw error;
    }
  };
  return {
    user: initial.user,
    token: initial.token,
    revision: initial.revision,
    loading: false,
    hydrated: false,
    hydrate: refresh,
    refreshMe: refresh,
    logout,
    login: (email, password) =>
      authenticate(() => api.loginUser({ email, password })),
    register: (payload) => authenticate(() => api.registerUser(payload)),
  };
});
