import React, { useEffect, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { LogIn, UserPlus, User, LogOut, KeyRound, Mail } from "lucide-react";
import { useAuthStore } from "../store/useAuthStore";
import { useToastStore } from "../store/useToastStore";
import { API_BASE_URL } from "../constants";

type AuthMode = "login" | "register" | "forgot" | "reset";

const AuthPage: React.FC = () => {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const { addToast } = useToastStore();
  const { user, loading, hydrated, hydrate, login, register, logout } = useAuthStore();

  const [mode, setMode] = useState<AuthMode>(() => {
    return searchParams.get("token") ? "reset" : "login";
  });
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [resetToken, setResetToken] = useState(() => searchParams.get("token") ?? "");
  const [newPassword, setNewPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [forgotSent, setForgotSent] = useState(false);

  useEffect(() => {
    if (!hydrated) hydrate();
  }, [hydrated, hydrate]);

  const handleLoginRegister = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      if (mode === "login") {
        await login(email.trim(), password);
        addToast("Signed in", "success");
      } else {
        await register({
          email: email.trim(),
          password,
          display_name: displayName.trim() || undefined,
        });
        addToast("Account created", "success");
      }
      navigate("/me/uploads");
    } catch (err) {
      addToast(err instanceof Error ? err.message : "Authentication failed", "error");
    }
  };

  const handleForgotPassword = async (e: React.FormEvent) => {
    e.preventDefault();
    setSubmitting(true);
    try {
      await fetch(`${API_BASE_URL}/api/v1/auth/forgot-password`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email: email.trim() }),
      });
      // Always show success — backend never reveals whether email exists
      setForgotSent(true);
    } catch {
      // Still show success to prevent email enumeration
      setForgotSent(true);
    } finally {
      setSubmitting(false);
    }
  };

  const handleResetPassword = async (e: React.FormEvent) => {
    e.preventDefault();
    if (newPassword.length < 8) {
      addToast("Password must be at least 8 characters", "error");
      return;
    }
    setSubmitting(true);
    try {
      const res = await fetch(`${API_BASE_URL}/api/v1/auth/reset-password`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ token: resetToken.trim(), new_password: newPassword }),
      });
      if (!res.ok) {
        const text = await res.text().catch(() => "");
        let message = `HTTP ${res.status}`;
        try {
          const json = JSON.parse(text);
          message = json.error || json.message || message;
        } catch {
          message = text || message;
        }
        throw new Error(message);
      }
      addToast("Password updated — please sign in", "success");
      setMode("login");
      setNewPassword("");
      setResetToken("");
    } catch (err) {
      addToast(err instanceof Error ? err.message : "Reset failed", "error");
    } finally {
      setSubmitting(false);
    }
  };

  if (user) {
    return (
      <>
        <Helmet>
          <title>Account | Through Your Letters</title>
        </Helmet>
        <div className="max-w-xl mx-auto py-20 space-y-8">
          <div className="card p-6 sm:p-10 shadow-card space-y-6">
            <div className="flex items-center gap-3">
              <User size={24} className="text-terracotta" />
              <h1 className="font-display font-normal text-3xl tracking-tight">Account</h1>
            </div>
            <div className="space-y-1">
              <p className="text-sm font-semibold uppercase">{user.display_name || user.email}</p>
              <p className="text-xs font-bold text-stone-500 uppercase tracking-widest">{user.email}</p>
            </div>
            <div className="flex flex-wrap gap-3">
              <Link
                to="/me/uploads"
                className="btn-dark"
              >
                My Uploads
              </Link>
              <Link
                to="/me/notifications"
                className="border border-line px-5 py-3 text-xs font-semibold uppercase hover:bg-ink hover:text-white transition-colors"
              >
                Notifications
              </Link>
              <button
                onClick={() => {
                  logout();
                  addToast("Signed out", "info");
                }}
                className="border border-line px-5 py-3 text-xs font-semibold uppercase text-red-600 hover:bg-red-600 hover:text-white transition-colors inline-flex items-center gap-2"
              >
                <LogOut size={14} /> Logout
              </button>
            </div>
          </div>
        </div>
      </>
    );
  }

  // ── Forgot password ────────────────────────────────────────────────────────

  if (mode === "forgot") {
    return (
      <>
        <Helmet>
          <title>Forgot Password | Through Your Letters</title>
        </Helmet>
        <div className="max-w-xl mx-auto py-20 space-y-8">
          {forgotSent ? (
            <div className="card p-6 sm:p-10 shadow-card space-y-6">
              <div className="flex items-center gap-3">
                <Mail size={22} className="text-terracotta" />
                <h1 className="font-display font-normal text-3xl tracking-tight">Check Your Email</h1>
              </div>
              <p className="text-sm font-bold leading-relaxed">
                If an account with that email exists, a password reset link has been sent.
              </p>
              <button
                onClick={() => { setMode("login"); setForgotSent(false); }}
                className="btn-dark w-full !py-4"
              >
                Back to Sign In
              </button>
            </div>
          ) : (
            <form onSubmit={handleForgotPassword} className="card p-6 sm:p-10 shadow-card space-y-6">
              <div className="flex items-center gap-3">
                <KeyRound size={22} className="text-terracotta" />
                <h1 className="font-display font-normal text-3xl tracking-tight">Forgot Password</h1>
              </div>
              <p className="text-xs font-bold text-stone-500 uppercase tracking-widest">
                Enter your email and we'll send a reset link.
              </p>
              <input
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="Email"
                required
                className="field w-full"
              />
              <button
                type="submit"
                disabled={submitting}
                className="btn-dark w-full !py-4"
              >
                {submitting ? "Sending..." : "Send Reset Link"}
              </button>
              <button
                type="button"
                onClick={() => setMode("login")}
                className="w-full text-xs font-semibold uppercase text-stone-500 hover:text-ink transition-colors"
              >
                Back to Sign In
              </button>
            </form>
          )}
        </div>
      </>
    );
  }

  // ── Reset password ─────────────────────────────────────────────────────────

  if (mode === "reset") {
    return (
      <>
        <Helmet>
          <title>Reset Password | Through Your Letters</title>
        </Helmet>
        <div className="max-w-xl mx-auto py-20 space-y-8">
          <form onSubmit={handleResetPassword} className="card p-6 sm:p-10 shadow-card space-y-6">
            <div className="flex items-center gap-3">
              <KeyRound size={22} className="text-terracotta" />
              <h1 className="font-display font-normal text-3xl tracking-tight">Reset Password</h1>
            </div>
            <input
              type="text"
              value={resetToken}
              onChange={(e) => setResetToken(e.target.value)}
              placeholder="Reset Token"
              required
              className="field w-full"
            />
            <input
              type="password"
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
              placeholder="New Password (min 8 characters)"
              required
              minLength={8}
              className="field w-full"
            />
            <button
              type="submit"
              disabled={submitting}
              className="btn-dark w-full !py-4"
            >
              {submitting ? "Updating..." : "Update Password"}
            </button>
            <button
              type="button"
              onClick={() => setMode("login")}
              className="w-full text-xs font-semibold uppercase text-stone-500 hover:text-ink transition-colors"
            >
              Back to Sign In
            </button>
          </form>
        </div>
      </>
    );
  }

  // ── Login / Register ───────────────────────────────────────────────────────

  return (
    <>
      <Helmet>
        <title>{mode === "login" ? "Sign In" : "Register"} | Through Your Letters</title>
      </Helmet>
      <div className="max-w-xl mx-auto py-20 space-y-8">
        <div className="flex gap-1 rounded-full border border-line bg-surface p-1">
          <button
            onClick={() => setMode("login")}
            className={`flex-1 rounded-full px-4 py-2 text-sm font-medium transition-colors whitespace-nowrap ${mode === "login" ? "bg-ink text-paper" : "text-muted hover:text-ink"}`}
          >
            Sign In
          </button>
          <button
            onClick={() => setMode("register")}
            className={`flex-1 rounded-full px-4 py-2 text-sm font-medium transition-colors whitespace-nowrap ${mode === "register" ? "bg-ink text-paper" : "text-muted hover:text-ink"}`}
          >
            Register
          </button>
        </div>

        <form onSubmit={handleLoginRegister} className="card p-6 sm:p-10 shadow-card space-y-6">
          <div className="flex items-center gap-3">
            {mode === "login" ? <LogIn size={22} className="text-terracotta" /> : <UserPlus size={22} className="text-terracotta" />}
            <h1 className="font-display font-normal text-3xl tracking-tight">
              {mode === "login" ? "Sign In" : "Create Account"}
            </h1>
          </div>

          {mode === "register" && (
            <input
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="Display Name (optional)"
              className="field w-full"
            />
          )}

          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="Email"
            required
            className="field w-full"
          />

          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Password"
            required
            minLength={8}
            className="field w-full"
          />

          <button
            type="submit"
            disabled={loading}
            className="btn-dark w-full !py-4"
          >
            {loading ? "Please wait..." : mode === "login" ? "Sign In" : "Create Account"}
          </button>

          {mode === "login" && (
            <button
              type="button"
              onClick={() => setMode("forgot")}
              className="w-full text-xs font-semibold uppercase text-stone-500 hover:text-ink transition-colors"
            >
              Forgot Password?
            </button>
          )}
        </form>
      </div>
    </>
  );
};

export default AuthPage;
