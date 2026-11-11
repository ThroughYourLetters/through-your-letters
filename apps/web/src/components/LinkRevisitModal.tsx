import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import { X, Link as LinkIcon, Search } from "lucide-react";
import { api, ApiError } from "../lib/api";
import type { Lettering } from "../types";
import ModalDialog from "./ModalDialog";
import { useDialogSession } from "./useDialogSession";

export default function LinkRevisitModal({
  originalId,
  onClose,
  onLinked,
}: {
  originalId: string;
  onClose: () => void;
  onLinked: () => void;
}) {
  const titleId = useId();
  const { current } = useDialogSession(onClose);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Lettering[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  const [notes, setNotes] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [savingId, setSavingId] = useState<string | null>(null);
  const saving = useRef(false);
  const search = useRef<AbortController | null>(null);
  useEffect(() => () => search.current?.abort(), []);

  async function handleSearch(event: FormEvent) {
    event.preventDefault();
    if (query.trim().length < 2 || saving.current) return;
    search.current?.abort();
    const controller = new AbortController();
    search.current = controller;
    setLoading(true);
    setSearched(false);
    setError(null);
    try {
      const data = await api.search(query.trim(), undefined, controller.signal);
      if (!controller.signal.aborted && current()) {
        setResults(data.filter((item) => item.id !== originalId));
        setSearched(true);
      }
    } catch (error) {
      if (!controller.signal.aborted && current())
        setError(
          error instanceof ApiError
            ? error.message
            : "Search could not be completed. Please try again.",
        );
    } finally {
      if (!controller.signal.aborted && current()) setLoading(false);
    }
  }

  async function link(targetId: string) {
    if (saving.current) return;
    saving.current = true;
    setSavingId(targetId);
    setError(null);
    try {
      await api.linkRevisit(originalId, {
        revisit_lettering_id: targetId,
        notes: notes.trim() || undefined,
      });
      if (current()) {
        onClose();
        onLinked();
      }
    } catch (error) {
      if (current())
        setError(
          error instanceof ApiError
            ? error.message
            : "The revisit could not be linked. Please try again.",
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
      className="card m-auto w-[calc(100%-2rem)] max-w-lg max-h-[calc(100dvh-2rem)] p-0 shadow-card"
    >
      <div className="sticky top-0 z-10 bg-ink text-white p-4 flex justify-between items-center gap-3">
        <h2
          id={titleId}
          className="font-semibold text-sm flex items-center gap-2"
        >
          <LinkIcon size={18} aria-hidden="true" /> Link a timeline revisit
        </h2>
        <button
          type="button"
          aria-label="Close revisit picker"
          onClick={onClose}
          className="p-2"
        >
          <X size={20} aria-hidden="true" />
        </button>
      </div>
      <div className="p-6 space-y-5">
        <form onSubmit={handleSearch} className="space-y-2">
          <label
            className="block text-sm font-semibold"
            htmlFor={`${titleId}-query`}
          >
            Search by text or postal code
          </label>
          <div className="flex gap-2">
            <input
              id={`${titleId}-query`}
              value={query}
              disabled={savingId !== null}
              minLength={2}
              required
              onChange={(event) => {
                search.current?.abort();
                setQuery(event.target.value);
                setResults([]);
                setSearched(false);
                setLoading(false);
                setError(null);
              }}
              className="field min-w-0 flex-1"
            />
            <button
              type="submit"
              aria-label="Search for a revisit"
              className="btn-dark !px-3"
              disabled={loading || savingId !== null || query.trim().length < 2}
            >
              <Search size={20} aria-hidden="true" />
            </button>
          </div>
        </form>
        <div className="space-y-2 text-sm font-semibold">
          <label className="block" htmlFor={`${titleId}-notes`}>
            What changed? (optional)
          </label>
          <textarea
            id={`${titleId}-notes`}
            value={notes}
            onChange={(event) => setNotes(event.target.value)}
            disabled={savingId !== null}
            placeholder="For example, repainted or faded"
            className="field min-h-20"
          />
        </div>
        {error && (
          <p role="alert" className="text-sm text-red-800">
            {error}
          </p>
        )}
        {loading && <p role="status">Searching…</p>}
        {searched && results.length === 0 && (
          <p role="status">No other letterings match this search.</p>
        )}
        <div className="space-y-3">
          {results.map((result) => (
            <button
              key={result.id}
              onClick={() => void link(result.id)}
              disabled={savingId !== null}
              className="w-full flex items-center gap-3 border border-line rounded-xl p-3 hover:bg-stone-50 disabled:opacity-50"
            >
              <img
                src={result.thumbnail_urls.small}
                className="w-12 h-12 shrink-0 object-cover"
                alt=""
              />
              <span className="min-w-0 text-left">
                <span className="block text-sm font-semibold break-words">
                  {result.detected_text || "Untitled lettering"}
                </span>
                <span className="block text-xs text-muted">
                  {result.pin_code} /{" "}
                  {new Date(result.created_at).toLocaleDateString()}
                </span>
              </span>
              <span className="ml-auto text-xs font-semibold text-terracotta-dark">
                {savingId === result.id ? "Linking…" : "Select"}
              </span>
            </button>
          ))}
        </div>
      </div>
    </ModalDialog>
  );
}
