import type { AuthUser } from "./api";

export const USER_SESSION_KEY = "ttl_user_token";
export const ADMIN_SESSION_KEY = "ttl_admin_token";
const ACCOUNT_KEY = "ttl_user_account";
let revision = 0;
let controller = new AbortController();

/** A verified account is paired with the exact credential that established it. */
export function captureUserSession() {
  const token = sessionStorage.getItem(USER_SESSION_KEY);
  let user: AuthUser | null = null;
  try {
    const saved = JSON.parse(sessionStorage.getItem(ACCOUNT_KEY) || "null");
    if (
      token &&
      saved?.token === token &&
      typeof saved.user?.id === "string" &&
      saved.user.id
    ) {
      user = saved.user;
    }
  } catch {
    /* Older or damaged metadata must never establish ownership. */
  }
  return { token, user, revision, signal: controller.signal };
}

export type UserSession = ReturnType<typeof captureUserSession>;

export function isCurrentUserSession(session: UserSession): boolean {
  return (
    !session.signal.aborted &&
    session.revision === revision &&
    session.token === sessionStorage.getItem(USER_SESSION_KEY)
  );
}

/** Cancel old account work synchronously, before its replacement is rendered. */
export function replaceUserSession(
  token: string | null,
  user: AuthUser | null,
): number {
  const previous = captureUserSession();
  if (token && user) {
    sessionStorage.setItem(ACCOUNT_KEY, JSON.stringify({ token, user }));
    sessionStorage.setItem(USER_SESSION_KEY, token);
  } else {
    sessionStorage.removeItem(USER_SESSION_KEY);
    sessionStorage.removeItem(ACCOUNT_KEY);
  }
  if (previous.token !== token || previous.user?.id !== user?.id) {
    controller.abort();
    controller = new AbortController();
    revision++;
  }
  return revision;
}

export function sessionChanged(): DOMException {
  return new DOMException(
    "Account changed; the previous request was cancelled",
    "AbortError",
  );
}
