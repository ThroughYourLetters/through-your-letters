import React, { useEffect, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useNavigate } from "react-router-dom";
import { LogIn, UserPlus, User, LogOut } from "lucide-react";
import { useAuthStore } from "../store/useAuthStore";
import { useToastStore } from "../store/useToastStore";

const AuthPage: React.FC = () => {
  const navigate = useNavigate();
  const { addToast } = useToastStore();
  const { user, loading, hydrated, hydrate, login, register, logout } = useAuthStore();

  const [mode, setMode] = useState<"login" | "register">("login");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");

  useEffect(() => {
    if (!hydrated) hydrate();
  }, [hydrated, hydrate]);

  const handleSubmit = async (e: React.FormEvent) => {
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

  if (user) {
    return (
      <>
        <Helmet>
          <title>Account | Through Your Letters</title>
        </Helmet>
        <div className="max-w-xl mx-auto py-20 space-y-8">
          <div className="border-4 border-black bg-white p-10 brutalist-shadow space-y-6">
            <div className="flex items-center gap-3">
              <User size={24} className="text-[#cc543a]" />
              <h1 className="text-3xl font-black uppercase tracking-tighter">Account</h1>
            </div>
            <div className="space-y-1">
              <p className="text-sm font-black uppercase">{user.display_name || user.email}</p>
              <p className="text-[10px] font-bold text-slate-500 uppercase tracking-widest">{user.email}</p>
            </div>
            <div className="flex flex-wrap gap-3">
              <Link
                to="/me/uploads"
                className="bg-black text-white px-5 py-3 text-[10px] font-black uppercase hover:bg-[#cc543a] transition-colors"
              >
                My Uploads
              </Link>
              <Link
                to="/me/notifications"
                className="border-2 border-black px-5 py-3 text-[10px] font-black uppercase hover:bg-black hover:text-white transition-colors"
              >
                Notifications
              </Link>
              <button
                onClick={() => {
                  logout();
                  addToast("Signed out", "info");
                }}
                className="border-2 border-black px-5 py-3 text-[10px] font-black uppercase text-red-600 hover:bg-red-600 hover:text-white transition-colors inline-flex items-center gap-2"
              >
                <LogOut size={14} /> Logout
              </button>
            </div>
          </div>
        </div>
      </>
    );
  }

  return (
    <>
      <Helmet>
        <title>{mode === "login" ? "Sign In" : "Register"} | Through Your Letters</title>
      </Helmet>
      <div className="max-w-xl mx-auto py-20 space-y-8">
        <div className="flex gap-2 border-2 border-black bg-white p-2">
          <button
            onClick={() => setMode("login")}
            className={`flex-1 py-3 text-[10px] font-black uppercase ${mode === "login" ? "bg-black text-white" : "bg-white"}`}
          >
            Sign In
          </button>
          <button
            onClick={() => setMode("register")}
            className={`flex-1 py-3 text-[10px] font-black uppercase ${mode === "register" ? "bg-black text-white" : "bg-white"}`}
          >
            Register
          </button>
        </div>

        <form onSubmit={handleSubmit} className="border-4 border-black bg-white p-10 brutalist-shadow space-y-6">
          <div className="flex items-center gap-3">
            {mode === "login" ? <LogIn size={22} className="text-[#cc543a]" /> : <UserPlus size={22} className="text-[#cc543a]" />}
            <h1 className="text-3xl font-black uppercase tracking-tighter">
              {mode === "login" ? "Sign In" : "Create Account"}
            </h1>
          </div>

          {mode === "register" && (
            <input
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder="Display Name (optional)"
              className="w-full border-2 border-black p-4 font-black text-sm outline-none focus:border-[#cc543a]"
            />
          )}

          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="Email"
            required
            className="w-full border-2 border-black p-4 font-black text-sm outline-none focus:border-[#cc543a]"
          />

          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Password"
            required
            minLength={8}
            className="w-full border-2 border-black p-4 font-black text-sm outline-none focus:border-[#cc543a]"
          />

          <button
            type="submit"
            disabled={loading}
            className="w-full bg-black text-white py-4 font-black text-[10px] uppercase tracking-widest hover:bg-[#cc543a] transition-colors disabled:opacity-50"
          >
            {loading ? "Please wait..." : mode === "login" ? "Sign In" : "Create Account"}
          </button>
        </form>
      </div>
    </>
  );
};

export default AuthPage;
