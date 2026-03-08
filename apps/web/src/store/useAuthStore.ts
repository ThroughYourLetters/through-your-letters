import { create } from "zustand";
import { api, AuthUser, USER_SESSION_KEY, registerUnauthorizedHandler } from "../lib/api";

interface AuthState {
  user: AuthUser | null;
  token: string | null;
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
  // Wire fetchJson's 401 handler so any 401 immediately clears the in-memory auth state.
  registerUnauthorizedHandler(() => {
    set({ user: null, token: null, hydrated: true, loading: false });
  });

  return {
  user: null,
  token: sessionStorage.getItem(USER_SESSION_KEY),
  loading: false,
  hydrated: false,

  hydrate: async () => {
    const token = sessionStorage.getItem(USER_SESSION_KEY);
    if (!token) {
      set({ user: null, token: null, hydrated: true });
      return;
    }

    set({ loading: true });
    try {
      const user = await api.getCurrentUser();
      set({ user, token, hydrated: true, loading: false });
    } catch {
      sessionStorage.removeItem(USER_SESSION_KEY);
      set({ user: null, token: null, hydrated: true, loading: false });
    }
  },

  login: async (email, password) => {
    set({ loading: true });
    try {
      const data = await api.loginUser({ email, password });
      set({ user: data.user, token: data.token, loading: false, hydrated: true });
    } catch (e) {
      set({ loading: false });
      throw e;
    }
  },

  register: async (payload) => {
    set({ loading: true });
    try {
      const data = await api.registerUser(payload);
      set({ user: data.user, token: data.token, loading: false, hydrated: true });
    } catch (e) {
      set({ loading: false });
      throw e;
    }
  },

  refreshMe: async () => {
    const token = sessionStorage.getItem(USER_SESSION_KEY);
    if (!token) {
      set({ user: null, token: null });
      return;
    }
    try {
      const user = await api.getCurrentUser();
      set({ user, token });
    } catch {
      sessionStorage.removeItem(USER_SESSION_KEY);
      set({ user: null, token: null });
    }
  },

  logout: () => {
    sessionStorage.removeItem(USER_SESSION_KEY);
    set({ user: null, token: null, loading: false, hydrated: true });
  },
  };
});
