import React, { useEffect } from "react";
import { Helmet } from "react-helmet-async";
import { Link } from "react-router-dom";
import { useInfiniteQuery, useMutation } from "@tanstack/react-query";
import { Bell, CheckCircle2, Loader2 } from "lucide-react";
import { api } from "../lib/api";
import { useAuthStore } from "../store/useAuthStore";
import { useToastStore } from "../store/useToastStore";
import { nextAccountPage } from "../lib/pagination";
import { captureUserSession, isCurrentUserSession } from "../lib/userSession";

const NotificationsPage: React.FC = () => {
  const { addToast } = useToastStore();
  const { user, hydrated, hydrate, revision } = useAuthStore();

  const notifications = useInfiniteQuery({
    queryKey: ["notifications", user?.id, revision],
    enabled: !!user,
    initialPageParam: 0 as number | string,
    queryFn: ({ pageParam, signal }) =>
      api.getNotifications({
        limit: 100,
        ...(typeof pageParam === "string"
          ? { after: pageParam }
          : { offset: pageParam }),
        signal,
      }),
    getNextPageParam: nextAccountPage,
  });
  const items = [
    ...new Map(
      (notifications.data?.pages.flatMap((page) => page.items) ?? []).map(
        (item) => [item.id, item],
      ),
    ).values(),
  ];
  const unread = notifications.data?.pages[0]?.unread;
  const loading = notifications.isPending;
  const session = captureUserSession();
  const markRead = useMutation({
    mutationFn: async (id: string) => {
      await api.markNotificationRead(id);
      if (isCurrentUserSession(session)) await notifications.refetch();
    },
    onError: () => {
      if (isCurrentUserSession(session))
        addToast(
          "The notification could not be marked as read. Please retry.",
          "error",
        );
    },
  });

  useEffect(() => {
    if (!hydrated) hydrate();
  }, [hydrated, hydrate]);

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
        <Link to="/auth" className="btn-dark">
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
        <div className="border-b border-line pb-8 flex flex-wrap gap-4 items-end justify-between">
          <div>
            <h1 className="font-display font-semibold text-4xl md:text-6xl tracking-tight">
              Notifications
            </h1>
            <p className="text-xs font-semibold uppercase text-stone-400 tracking-widest mt-2">
              Moderation and account updates
            </p>
          </div>
          {unread !== undefined && (
            <span className="text-xs font-semibold uppercase bg-ink text-white px-3 py-2">
              Unread {unread}
            </span>
          )}
        </div>

        {notifications.error && (
          <p role="alert">
            Notifications could not be loaded.{" "}
            <button
              className="underline"
              onClick={() =>
                void (notifications.isFetchNextPageError
                  ? notifications.fetchNextPage()
                  : notifications.refetch())
              }
            >
              Try again
            </button>
          </p>
        )}
        {loading ? (
          <div className="flex justify-center py-20">
            <Loader2 size={32} className="animate-spin text-terracotta" />
          </div>
        ) : items.length === 0 && !notifications.error ? (
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
                <div className="flex flex-wrap items-start justify-between gap-4">
                  <div className="min-w-0 break-words space-y-1">
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
                      disabled={markRead.isPending}
                      onClick={() => markRead.mutate(item.id)}
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
        {notifications.hasNextPage && (
          <button
            className="btn-outline"
            disabled={notifications.isFetchingNextPage}
            onClick={() => void notifications.fetchNextPage()}
          >
            {notifications.isFetchingNextPage
              ? "Loading more notifications…"
              : "Load more notifications"}
          </button>
        )}
      </div>
    </>
  );
};

export default NotificationsPage;
