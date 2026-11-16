import React, { useEffect, useId, useRef, useState } from "react";
import { Helmet } from "react-helmet-async";
import {
  Link,
  useLocation,
  useNavigate,
  useSearchParams,
} from "react-router-dom";
import { LogIn, UserPlus, User, LogOut, KeyRound, Mail } from "lucide-react";
import { useAuthStore } from "../store/useAuthStore";
import { useToastStore } from "../store/useToastStore";
import { api } from "../lib/api";

type AuthMode = "login" | "register" | "forgot" | "reset";

function returnDestination(candidate: unknown): string {
  if (typeof candidate === "string") {
    try {
      const destination = new URL(candidate, window.location.href);
      if (
        destination.protocol === window.location.protocol &&
        destination.host === window.location.host
      )
        return destination.pathname + destination.search + destination.hash;
    } catch {
      /* Invalid return destinations use the account workspace. */
    }
  }
  return "/me/uploads";
}

const AuthPage: React.FC = () => {
  const navigate = useNavigate();
  const location = useLocation();
  const [searchParams] = useSearchParams();
  const urlToken = searchParams.get("token") ?? "";
  const { addToast } = useToastStore();
  const { user, hydrated, hydrate, login, register, logout } = useAuthStore();
  const [mode, setMode] = useState<AuthMode>(urlToken ? "reset" : "login");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [resetToken, setResetToken] = useState(urlToken);
  const [newPassword, setNewPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [forgotSent, setForgotSent] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState(
    location.state?.passwordReset
      ? "Password updated. Please sign in with your new password."
      : "",
  );
  const request = useRef<AbortController | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const errorRef = useRef<HTMLParagraphElement>(null);
  const previousView = useRef(`${mode}:${forgotSent}`);
  const id = useId();

  useEffect(() => {
    if (!hydrated) void hydrate();
  }, [hydrated, hydrate]);

  useEffect(() => () => request.current?.abort(), []);

  useEffect(() => {
    if (location.state?.passwordReset) {
      setNotice("Password updated. Please sign in with your new password.");
    }
  }, [location.state?.passwordReset]);

  useEffect(() => {
    request.current?.abort();
    request.current = null;
    setSubmitting(false);
    setError("");
    setResetToken(urlToken);
    setMode((previous) =>
      urlToken ? "reset" : previous === "reset" ? "login" : previous,
    );
  }, [urlToken]);

  useEffect(() => {
    const view = `${mode}:${forgotSent}`;
    if (previousView.current !== view) heading.current?.focus();
    previousView.current = view;
  }, [mode, forgotSent]);

  useEffect(() => {
    if (error) errorRef.current?.focus();
  }, [error]);

  const switchMode = (next: AuthMode) => {
    request.current?.abort();
    request.current = null;
    setSubmitting(false);
    setError("");
    setNotice("");
    setForgotSent(false);
    setPassword("");
    setNewPassword("");
    setMode(next);
    if (urlToken) {
      const params = new URLSearchParams(location.search);
      params.delete("token");
      navigate(
        {
          pathname: location.pathname,
          search: params.toString(),
          hash: location.hash,
        },
        {
          replace: true,
          state: location.state,
        },
      );
    }
  };

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (request.current) return;
    const controller = new AbortController();
    request.current = controller;
    const href = window.location.href;
    setSubmitting(true);
    setError("");
    setNotice("");
    try {
      if (mode === "forgot") {
        await api.requestPasswordReset(email.trim(), controller.signal);
        controller.signal.throwIfAborted();
        setForgotSent(true);
      } else if (mode === "reset") {
        await api.resetPassword(
          resetToken.trim(),
          newPassword,
          controller.signal,
        );
        controller.signal.throwIfAborted();
        const params = new URLSearchParams(location.search);
        params.delete("token");
        navigate(
          {
            pathname: location.pathname,
            search: params.toString(),
            hash: location.hash,
          },
          {
            replace: true,
            state: { ...location.state, passwordReset: true },
          },
        );
        setMode("login");
        setNewPassword("");
        setResetToken("");
        setNotice("Password updated. Please sign in with your new password.");
        logout();
        addToast("Password updated — please sign in", "success");
      } else {
        if (mode === "login")
          await login(email.trim(), password, controller.signal);
        else
          await register(
            {
              email: email.trim(),
              password,
              display_name: displayName.trim() || undefined,
            },
            controller.signal,
          );
        // Signing in remounts account views; navigation still belongs to this form.
        if (window.location.href === href) {
          addToast(
            mode === "login" ? "Signed in" : "Account created",
            "success",
          );
          navigate(returnDestination(location.state?.from));
        }
      }
    } catch (err) {
      if (!controller.signal.aborted)
        setError(
          err instanceof Error
            ? err.message
            : "The request failed. Please try again.",
        );
    } finally {
      if (request.current === controller) {
        request.current = null;
        setSubmitting(false);
      }
    }
  };

  if (user && mode !== "reset") {
    return (
      <>
        <Helmet>
          <title>Account | Through Your Letters</title>
        </Helmet>
        <div className="max-w-xl mx-auto py-20 space-y-8">
          <div className="card p-6 sm:p-10 shadow-card space-y-6">
            <div className="flex items-center gap-3">
              <User size={24} className="text-terracotta shrink-0" />
              <h1 className="font-display font-semibold text-3xl tracking-tight">
                Account
              </h1>
            </div>
            <div className="space-y-1 break-words">
              <p className="text-sm font-semibold uppercase">
                {user.display_name || user.email}
              </p>
              <p className="text-xs font-bold text-muted uppercase tracking-widest">
                {user.email}
              </p>
            </div>
            <div className="flex flex-wrap gap-3">
              <Link to="/me/uploads" className="btn-dark">
                My Uploads
              </Link>
              <Link to="/me/notifications" className="btn-secondary">
                Notifications
              </Link>
              <button
                onClick={() => {
                  logout();
                  addToast("Signed out", "info");
                }}
                className="btn-secondary"
              >
                <LogOut size={14} /> Logout
              </button>
            </div>
          </div>
        </div>
      </>
    );
  }

  const title = forgotSent
    ? "Check Your Email"
    : {
        login: "Sign In",
        register: "Create Account",
        forgot: "Forgot Password",
        reset: "Reset Password",
      }[mode];
  const Icon = forgotSent
    ? Mail
    : mode === "login"
      ? LogIn
      : mode === "register"
        ? UserPlus
        : KeyRound;
  const submitLabel = {
    login: "Sign In",
    register: "Create Account",
    forgot: "Send Reset Link",
    reset: "Update Password",
  }[mode];
  const pendingLabel = {
    login: "Please wait...",
    register: "Please wait...",
    forgot: "Sending...",
    reset: "Updating...",
  }[mode];

  return (
    <>
      <Helmet>
        <title>{title} | Through Your Letters</title>
      </Helmet>
      <div className="max-w-xl mx-auto py-20 space-y-8">
        {(mode === "login" || mode === "register") && (
          <div
            role="group"
            className="flex gap-1 rounded-full border border-line bg-surface p-1"
            aria-label="Account access"
          >
            {(["login", "register"] as const).map((value) => (
              <button
                key={value}
                type="button"
                aria-pressed={mode === value}
                onClick={() => switchMode(value)}
                className={`flex-1 rounded-full px-4 py-2 text-sm font-medium transition-colors whitespace-nowrap ${mode === value ? "bg-ink text-paper" : "text-muted hover:text-ink"}`}
              >
                {value === "login" ? "Sign In" : "Register"}
              </button>
            ))}
          </div>
        )}
        <form
          onSubmit={handleSubmit}
          aria-labelledby={`${id}-title`}
          aria-busy={submitting}
          className="card p-6 sm:p-10 shadow-card space-y-6"
        >
          <div className="flex items-center gap-3">
            <Icon size={22} className="text-terracotta shrink-0" />
            <h1
              id={`${id}-title`}
              ref={heading}
              tabIndex={-1}
              className="font-display font-semibold text-3xl tracking-tight"
            >
              {title}
            </h1>
          </div>
          {error && (
            <p
              ref={errorRef}
              tabIndex={-1}
              role="alert"
              className="text-red-700 text-sm break-words"
            >
              {error}
            </p>
          )}
          {notice && (
            <p role="status" className="text-sm">
              {notice}
            </p>
          )}
          {forgotSent ? (
            <p role="status" className="text-sm leading-relaxed">
              If an account with that email exists, a password reset link has
              been sent.
            </p>
          ) : (
            <>
              {mode === "forgot" && (
                <p className="text-sm text-muted">
                  Enter your email and we'll send a reset link.
                </p>
              )}
              <fieldset disabled={submitting} className="min-w-0 space-y-6">
                {mode === "register" && (
                  <label className="block text-sm font-medium">
                    Display name (optional)
                    <input
                      value={displayName}
                      onChange={(e) => setDisplayName(e.target.value)}
                      placeholder="Display Name (optional)"
                      autoComplete="nickname"
                      className="field w-full mt-2 !text-base"
                    />
                  </label>
                )}
                {mode !== "reset" ? (
                  <label className="block text-sm font-medium">
                    Email
                    <input
                      type="email"
                      name="email"
                      value={email}
                      onChange={(e) => setEmail(e.target.value)}
                      placeholder="Email"
                      autoComplete={mode === "forgot" ? "email" : "username"}
                      required
                      className="field w-full mt-2 !text-base"
                    />
                  </label>
                ) : (
                  <label className="block text-sm font-medium">
                    Reset token
                    <input
                      type="password"
                      value={resetToken}
                      onChange={(e) => setResetToken(e.target.value)}
                      placeholder="Reset Token"
                      autoComplete="off"
                      spellCheck={false}
                      required
                      className="field w-full mt-2 !text-base"
                    />
                  </label>
                )}
                {mode !== "forgot" && (
                  <div className="text-sm font-medium">
                    <label htmlFor={`${id}-password`}>
                      {mode === "reset" ? "New password" : "Password"}
                    </label>
                    <input
                      id={`${id}-password`}
                      type="password"
                      name="password"
                      value={mode === "reset" ? newPassword : password}
                      onChange={(e) =>
                        mode === "reset"
                          ? setNewPassword(e.target.value)
                          : setPassword(e.target.value)
                      }
                      placeholder={
                        mode === "reset"
                          ? "New Password (min 8 characters)"
                          : "Password"
                      }
                      autoComplete={
                        mode === "login" ? "current-password" : "new-password"
                      }
                      required
                      minLength={mode === "login" ? undefined : 8}
                      aria-describedby={
                        mode === "login" ? undefined : `${id}-password-help`
                      }
                      className="field w-full mt-2 !text-base"
                    />
                    {mode !== "login" && (
                      <span
                        id={`${id}-password-help`}
                        className="block text-sm text-muted mt-2"
                      >
                        Use at least 8 characters.
                      </span>
                    )}
                  </div>
                )}
              </fieldset>
              <button
                type="submit"
                disabled={submitting}
                className="btn-dark w-full !py-4"
              >
                {submitting ? pendingLabel : submitLabel}
              </button>
            </>
          )}
          {mode === "login" && (
            <button
              type="button"
              onClick={() => switchMode("forgot")}
              className="w-full text-sm font-semibold text-muted hover:text-ink"
            >
              Forgot Password?
            </button>
          )}
          {(mode === "forgot" || mode === "reset") && (
            <button
              type="button"
              onClick={() => switchMode("login")}
              className="w-full text-sm font-semibold text-muted hover:text-ink"
            >
              Back to Sign In
            </button>
          )}
        </form>
      </div>
    </>
  );
};

export default AuthPage;
