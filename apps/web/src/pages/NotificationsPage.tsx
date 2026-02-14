import React, { useCallback, useEffect, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useNavigate } from "react-router-dom";
import { Bell, CheckCircle2, Loader2 } from "lucide-react";
import { api, NotificationItem } from "../lib/api";
import { useAuthStore } from "../store/useAuthStore";
import { useToastStore } from "../store/useToastStore";

const NotificationsPage: React.FC = () => {
  const navigate = useNavigate();
  const { addToast } = useToastStore();
  const { user, hydrated, hydrate } = useAuthStore();

  const [items, setItems] = useState<NotificationItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [unread, setUnread] = useState(0);

  useEffect(() => {
    if (!hydrated) hydrate();
  }, [hydrated, hydrate]);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const data = await api.getNotifications({ limit: 100, offset: 0 });
      setItems(data.items);
      setUnread(data.unread);
    } catch (err) {
      addToast(
        err instanceof Error ? err.message : "Failed to load notifications",
        "error",
      );
      navigate("/auth");
    } finally {
      setLoading(false);
    }
  }, [addToast, navigate]);

  useEffect(() => {
    if (user) load();
  }, [user, load]);

  const markRead = async (id: string) => {
    try {
      await api.markNotificationRead(id);
      setItems((prev) =>
        prev.map((item) =>
          item.id === id ? { ...item, is_read: true } : item,
        ),
      );
      setUnread((u) => Math.max(0, u - 1));
    } catch (err) {
      addToast(
        err instanceof Error ? err.message : "Failed to mark as read",
        "error",
      );
    }
  };

  if (!hydrated) {
    return (
      <div className="flex justify-center py-20">
        <Loader2 size={32} className="animate-spin text-[#cc543a]" />
      </div>
    );
  }

  if (!user) {
    return (
      <div className="max-w-xl mx-auto py-20 text-center space-y-6">
        <h1 className="text-4xl font-black uppercase tracking-tighter">
          Sign In Required
        </h1>
        <p className="text-slate-500">
          You need an account to see notifications.
        </p>
        <Link
          to="/auth"
          className="bg-black text-white px-6 py-3 text-[10px] font-black uppercase hover:bg-[#cc543a] transition-colors"
        >
          Go to Auth
        </Link>
      </div>
    );
  }

  return (
    <>
      <Helmet>
        <title>Notifications | Through Your Letters</title>
      </Helmet>
      <div className="space-y-10 pb-24">
        <div className="border-b-4 border-black pb-8 flex items-end justify-between">
          <div>
            <h1 className="text-4xl md:text-6xl font-black uppercase tracking-tighter">
              Notifications
            </h1>
            <p className="text-[10px] font-black uppercase text-slate-400 tracking-widest mt-2">
              Moderation and account updates
            </p>
          </div>
          <span className="text-[10px] font-black uppercase bg-black text-white px-3 py-2">
            Unread {unread}
          </span>
        </div>

        {loading ? (
          <div className="flex justify-center py-20">
            <Loader2 size={32} className="animate-spin text-[#cc543a]" />
          </div>
        ) : items.length === 0 ? (
          <div className="text-center py-20 border-4 border-dashed border-black/20 text-slate-400 font-black uppercase">
            No notifications
          </div>
        ) : (
          <div className="grid gap-4">
            {items.map((item) => (
              <div
                key={item.id}
                className={`border-2 border-black p-5 bg-white ${item.is_read ? "opacity-70" : "brutalist-shadow-sm"}`}
              >
                <div className="flex items-start justify-between gap-4">
                  <div className="space-y-1">
                    <p className="text-[10px] font-black uppercase text-[#cc543a] tracking-widest inline-flex items-center gap-2">
                      <Bell size={12} /> {item.type}
                    </p>
                    <h3 className="text-lg font-black uppercase tracking-tight">
                      {item.title}
                    </h3>
                    {item.body && (
                      <p className="text-sm text-slate-700">{item.body}</p>
                    )}
                    <p className="text-[9px] font-bold text-slate-500 uppercase">
                      {new Date(item.created_at).toLocaleString()}
                    </p>
                  </div>

                  {!item.is_read && (
                    <button
                      onClick={() => markRead(item.id)}
                      className="inline-flex items-center gap-2 border-2 border-black px-3 py-2 text-[10px] font-black uppercase hover:bg-black hover:text-white transition-colors"
                    >
                      <CheckCircle2 size={14} /> Mark Read
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </>
  );
};

export default NotificationsPage;
