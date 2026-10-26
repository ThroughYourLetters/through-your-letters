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
        <Loader2 size={32} className="animate-spin text-terracotta" />
      </div>
    );
  }

  if (!user) {
    return (
      <div className="max-w-xl mx-auto py-20 text-center space-y-6">
        <h1 className="font-display font-semibold text-4xl tracking-tight">
          Sign In Required
        </h1>
        <p className="text-stone-500">
          You need an account to see notifications.
        </p>
        <Link
          to="/auth"
          className="btn-dark"
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
        <div className="border-b border-line pb-8 flex items-end justify-between">
          <div>
            <h1 className="font-display font-semibold text-4xl md:text-6xl tracking-tight">
              Notifications
            </h1>
            <p className="text-xs font-semibold uppercase text-stone-400 tracking-widest mt-2">
              Moderation and account updates
            </p>
          </div>
          <span className="text-xs font-semibold uppercase bg-ink text-white px-3 py-2">
            Unread {unread}
          </span>
        </div>

        {loading ? (
          <div className="flex justify-center py-20">
            <Loader2 size={32} className="animate-spin text-terracotta" />
          </div>
        ) : items.length === 0 ? (
          <div className="text-center py-20 border border-dashed border-line text-stone-400 font-semibold uppercase">
            No notifications
          </div>
        ) : (
          <div className="grid gap-4">
            {items.map((item) => (
              <div
                key={item.id}
                className={`border border-line p-5 bg-surface ${item.is_read ? "opacity-70" : "shadow-soft"}`}
              >
                <div className="flex items-start justify-between gap-4">
                  <div className="space-y-1">
                    <p className="text-xs font-semibold uppercase text-terracotta tracking-widest inline-flex items-center gap-2">
                      <Bell size={12} /> {item.type}
                    </p>
                    <h3 className="text-lg font-semibold uppercase tracking-tight">
                      {item.title}
                    </h3>
                    {item.body && (
                      <p className="text-sm text-stone-700">{item.body}</p>
                    )}
                    <p className="text-xs font-bold text-stone-500 uppercase">
                      {new Date(item.created_at).toLocaleString()}
                    </p>
                  </div>

                  {!item.is_read && (
                    <button
                      onClick={() => markRead(item.id)}
                      className="inline-flex items-center gap-2 border border-line px-3 py-2 text-xs font-semibold uppercase hover:bg-ink hover:text-white transition-colors"
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
