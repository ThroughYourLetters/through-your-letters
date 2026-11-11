import { useId, useRef, useState } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { X, Plus, Loader2, FolderPlus } from "lucide-react";
import { api, ApiError } from "../lib/api";
import ModalDialog from "./ModalDialog";
import { useDialogSession } from "./useDialogSession";

export default function AddToCollectionModal({
  letteringId,
  onClose,
}: {
  letteringId: string;
  onClose: () => void;
}) {
  const titleId = useId();
  const { session, current } = useDialogSession(onClose);
  const [savingId, setSavingId] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const saving = useRef(false);
  const collections = useInfiniteQuery({
    queryKey: ["collections", "owned", session.user?.id, session.revision],
    enabled: !!session.user,
    initialPageParam: 0,
    queryFn: ({ pageParam, signal }) =>
      api.getMyCollections({
        limit: 20,
        offset: pageParam,
        signal,
      }),
    getNextPageParam: (last) =>
      last.offset + last.limit < last.total
        ? last.offset + last.limit
        : undefined,
    retry: false,
  });
  const items = [
    ...new Map(
      collections.data?.pages
        .flatMap((page) => page.items)
        .map((item) => [item.id, item]) ?? [],
    ).values(),
  ];

  async function add(id: string) {
    if (saving.current) return;
    saving.current = true;
    setSavingId(id);
    setSaveError(null);
    try {
      await api.addToCollection(id, letteringId);
      if (current()) onClose();
    } catch (error) {
      if (current())
        setSaveError(
          error instanceof ApiError
            ? error.message
            : "The lettering could not be saved. Please try again.",
        );
    } finally {
      saving.current = false;
      if (current()) setSavingId(null);
    }
  }

  return (
    <ModalDialog
      titleId={titleId}
      onClose={onClose}
      className="card m-auto w-[calc(100%-2rem)] max-w-md max-h-[calc(100dvh-2rem)] p-0 shadow-card"
    >
      <div className="sticky top-0 z-10 bg-ink text-white p-4 flex justify-between items-center gap-3">
        <h2
          id={titleId}
          className="font-semibold text-sm flex items-center gap-2"
        >
          <FolderPlus size={18} aria-hidden="true" /> Save to a collection
        </h2>
        <button
          type="button"
          aria-label="Close collection picker"
          onClick={onClose}
          className="p-2"
        >
          <X size={20} aria-hidden="true" />
        </button>
      </div>
      <div className="p-6 space-y-4">
        {collections.isPending && (
          <p role="status">Loading your collections…</p>
        )}
        {collections.isError && (
          <div role="alert" className="space-y-2 text-red-800">
            <p>Your collections could not be loaded.</p>
            <button
              className="underline"
              onClick={() =>
                void (collections.isFetchNextPageError
                  ? collections.fetchNextPage({ cancelRefetch: false })
                  : collections.refetch())
              }
            >
              Try again
            </button>
          </div>
        )}
        {saveError && (
          <p role="alert" className="text-red-800">
            {saveError}
          </p>
        )}
        {!collections.isPending &&
          !collections.isError &&
          items.length === 0 && (
            <p>
              You have no collections yet.{" "}
              <Link className="underline" to="/community" onClick={onClose}>
                Open Community
              </Link>{" "}
              and create one from the Collections tab.
            </p>
          )}
        {items.map((collection) => (
          <button
            key={collection.id}
            onClick={() => void add(collection.id)}
            disabled={savingId !== null}
            className="w-full border border-line rounded-xl p-4 text-left text-sm font-semibold flex justify-between items-center gap-3 hover:bg-stone-50 disabled:opacity-50"
          >
            <span className="min-w-0 break-words">
              {collection.name}
              {!collection.is_public && (
                <span className="block text-xs text-muted">Private</span>
              )}
            </span>
            {savingId === collection.id ? (
              <span className="flex items-center gap-2">
                <Loader2
                  size={14}
                  className="animate-spin"
                  aria-hidden="true"
                />{" "}
                Saving…
              </span>
            ) : (
              <Plus size={16} className="shrink-0" aria-hidden="true" />
            )}
          </button>
        ))}
        {collections.hasNextPage && (
          <button
            className="btn-secondary w-full"
            disabled={collections.isFetching || savingId !== null}
            onClick={() =>
              void collections.fetchNextPage({ cancelRefetch: false })
            }
          >
            {collections.isFetchingNextPage ? "Loading…" : "More collections"}
          </button>
        )}
      </div>
    </ModalDialog>
  );
}
