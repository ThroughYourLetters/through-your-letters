import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import ModalDialog from "./ModalDialog";
import { api } from "../lib/api";
import { useDialogSession } from "./useDialogSession";

export default function CreateCollectionDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: () => void;
}) {
  const id = useId();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [nameError, setNameError] = useState("");
  const nameRef = useRef<HTMLInputElement>(null);
  const errorRef = useRef<HTMLParagraphElement>(null);
  const pending = useRef<AbortController | null>(null);
  const { current } = useDialogSession(onClose);
  useEffect(() => () => pending.current?.abort(), []);
  useEffect(() => {
    if (error) errorRef.current?.focus();
  }, [error]);
  useEffect(() => {
    if (nameError) nameRef.current?.focus();
  }, [nameError]);
  async function create(event: FormEvent) {
    event.preventDefault();
    if (pending.current || !current()) return;
    if (!name.trim()) {
      setError("");
      setNameError("Enter a collection name.");
      return;
    }
    const controller = new AbortController();
    pending.current = controller;
    setBusy(true);
    setNameError("");
    setError("");
    try {
      await api.createCollection(
        { name: name.trim(), description: description.trim() },
        controller.signal,
      );
      if (current() && !controller.signal.aborted) onCreated();
    } catch (failure) {
      if (current() && !controller.signal.aborted)
        setError(
          failure instanceof Error
            ? failure.message
            : "The collection could not be created. Please try again.",
        );
    } finally {
      pending.current = null;
      if (current()) setBusy(false);
    }
  }
  return (
    <ModalDialog
      titleId={`${id}-title`}
      onClose={onClose}
      className="w-[calc(100%-2rem)] max-w-lg max-h-[calc(100dvh-2rem)] overflow-y-auto rounded-2xl border border-line bg-surface p-6 text-ink"
    >
      <form
        onSubmit={(event) => void create(event)}
        aria-labelledby={`${id}-title`}
        aria-busy={busy}
        className="space-y-5"
      >
        <h2 id={`${id}-title`} className="font-display text-2xl">
          New collection
        </h2>
        {error && (
          <p
            ref={errorRef}
            tabIndex={-1}
            role="alert"
            className="text-sm text-red-700 break-words"
          >
            {error}
          </p>
        )}
        <fieldset disabled={busy} className="min-w-0 space-y-4">
          <div className="text-sm font-medium">
            <label htmlFor={`${id}-name`}>Collection name</label>
            <input
              id={`${id}-name`}
              ref={nameRef}
              aria-invalid={!!nameError}
              aria-describedby={nameError ? `${id}-name-error` : undefined}
              value={name}
              onChange={(event) => {
                setName(event.target.value);
                setNameError("");
              }}
              placeholder="Collection Name"
              required
              className="field mt-2 !text-base"
            />
            {nameError && (
              <span
                id={`${id}-name-error`}
                role="alert"
                className="block mt-2 text-sm text-red-700"
              >
                {nameError}
              </span>
            )}
          </div>
          <label className="block text-sm font-medium">
            Description (optional)
            <textarea
              value={description}
              onChange={(event) => setDescription(event.target.value)}
              placeholder="Description (optional)"
              rows={3}
              className="field mt-2 !text-base"
            />
          </label>
        </fieldset>
        <div className="flex flex-wrap gap-3">
          <button type="submit" disabled={busy} className="btn-primary">
            {busy ? "Creating…" : "Create"}
          </button>
          <button type="button" onClick={onClose} className="btn-secondary">
            Cancel
          </button>
        </div>
      </form>
    </ModalDialog>
  );
}
