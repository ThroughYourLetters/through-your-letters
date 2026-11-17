import type { ChosenLocation } from "@ttl/types";
import LocationPicker from "./LocationPicker";
import React, { useEffect, useId, useState, useRef } from "react";
import { Link } from "react-router-dom";
import { postalCodeSchema } from "@ttl/validation";
import {
  Upload,
  X,
  Loader2,
  MapPin,
  Camera,
  CheckCircle,
  AlertCircle,
} from "lucide-react";
import { useToastStore } from "../store/useToastStore";
import { useCityStore } from "../store/useCityStore";
import { enqueueUpload } from "../lib/offlineQueue";
import { api } from "../lib/api";
import { captureUserSession, isCurrentUserSession } from "../lib/userSession";

type FileStatus = "pending" | "uploading" | "done" | "saved" | "error";

interface FileEntry {
  requestKey: string;
  requestSignature?: string;
  file: File;
  preview: string;
  status: FileStatus;
  error?: string;
}

const HEIC_MIME_TYPES = new Set([
  "image/heic",
  "image/heif",
  "image/heic-sequence",
  "image/heif-sequence",
]);

const HEIC_EXTENSION_REGEX = /\.(heic|heif)$/i;

const ContributionPanel: React.FC<{
  onCancel: () => void;
  onSubmit: () => void;
}> = ({ onCancel, onSubmit }) => {
  const { addToast } = useToastStore();
  const { selectedCityId, centerLat, centerLng } = useCityStore();
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [chosenLocation, setChosenLocation] = useState<ChosenLocation | null>(
    null,
  );
  const [showLocationPicker, setShowLocationPicker] = useState(false);
  const [formError, setFormError] = useState("");
  const uploadInProgress = useRef(false);
  const formRef = useRef<HTMLFormElement>(null);
  const errorRef = useRef<HTMLParagraphElement>(null);
  const id = useId();

  useEffect(() => {
    if (formError) errorRef.current?.focus();
  }, [formError]);

  const [form, setForm] = useState({
    name: "",
    pin: "",
    desc: "",
  });
  const fileRef = useRef<HTMLInputElement>(null);
  const cameraRef = useRef<HTMLInputElement>(null);

  const isHeicLikeFile = (file: File) =>
    HEIC_MIME_TYPES.has(file.type.toLowerCase()) ||
    HEIC_EXTENSION_REGEX.test(file.name);

  const convertToJpeg = async (file: File): Promise<File> => {
    const objectUrl = URL.createObjectURL(file);
    try {
      const image = await new Promise<HTMLImageElement>((resolve, reject) => {
        const img = new Image();
        img.onload = () => resolve(img);
        img.onerror = () => reject(new Error("Image decode failed"));
        img.src = objectUrl;
      });

      const canvas = document.createElement("canvas");
      canvas.width = image.naturalWidth || image.width;
      canvas.height = image.naturalHeight || image.height;

      const context = canvas.getContext("2d");
      if (!context) {
        throw new Error("Canvas context unavailable");
      }

      context.drawImage(image, 0, 0);

      const jpegBlob = await new Promise<Blob>((resolve, reject) => {
        canvas.toBlob(
          (blob) => {
            if (blob) resolve(blob);
            else reject(new Error("JPEG conversion failed"));
          },
          "image/jpeg",
          0.92,
        );
      });

      const baseName = file.name.replace(HEIC_EXTENSION_REGEX, "") || "image";
      return new File([jpegBlob], `${baseName}.jpg`, {
        type: "image/jpeg",
        lastModified: file.lastModified,
      });
    } finally {
      URL.revokeObjectURL(objectUrl);
    }
  };

  const addFiles = async (newFiles: FileList | null) => {
    if (!newFiles) return;

    const processedFiles = await Promise.all(
      Array.from(newFiles).map(async (file) => {
        if (!isHeicLikeFile(file)) return file;

        try {
          return await convertToJpeg(file);
        } catch {
          addToast(
            `Could not convert "${file.name}" from HEIC. Please switch iPhone camera format to Most Compatible (JPEG).`,
            "error",
          );
          return file;
        }
      }),
    );

    const entries: FileEntry[] = processedFiles.map((file) => ({
      requestKey: crypto.randomUUID(),
      file,
      preview: URL.createObjectURL(file),
      status: "pending" as FileStatus,
    }));
    setFiles((prev) => [...prev, ...entries]);
  };

  const removeFile = (idx: number) => {
    setFiles((prev) => {
      URL.revokeObjectURL(prev[idx].preview);
      return prev.filter((_, i) => i !== idx);
    });
  };

  const formatErrorLabel = (error?: string) => {
    if (!error) return "";
    return error.length > 48 ? `${error.slice(0, 45)}...` : error;
  };

  const handleUpload = async (e: React.FormEvent, saveForLater = false) => {
    e.preventDefault();
    if (uploadInProgress.current || !formRef.current?.reportValidity()) return;
    uploadInProgress.current = true;
    setFormError("");
    try {
      await uploadFiles(saveForLater);
    } catch (error) {
      setFormError(
        error instanceof Error
          ? error.message
          : "The photos could not be submitted. Please try again.",
      );
    } finally {
      uploadInProgress.current = false;
      setLoading(false);
    }
  };

  const uploadFiles = async (saveForLater: boolean) => {
    const session = captureUserSession();
    if (files.length === 0) return setFormError("At least one image required");
    if (!form.name.trim()) return setFormError("Contributor name required");
    if (!selectedCityId)
      return setFormError("Please select a city before uploading");
    const postal = postalCodeSchema.safeParse(form.pin);
    if (!postal.success) return setFormError(postal.error.issues[0].message);
    const requestSignature = JSON.stringify([
      form.name.trim(),
      postal.data,
      form.desc.trim(),
      selectedCityId,
      chosenLocation,
    ]);
    const submissions = files.map((entry) =>
      entry.requestSignature === requestSignature ||
      entry.status === "done" ||
      entry.status === "saved"
        ? entry
        : { ...entry, requestKey: crypto.randomUUID(), requestSignature },
    );
    setFiles(submissions);
    if (
      submissions.every(
        (entry) => entry.status === "done" || entry.status === "saved",
      )
    ) {
      onSubmit();
      return;
    }
    setLoading(true);

    if (saveForLater || !navigator.onLine) {
      let queued = 0;
      let failed = 0;
      for (const entry of submissions) {
        if (entry.status === "done" || entry.status === "saved") continue;
        if (!isCurrentUserSession(session)) return;
        try {
          await enqueueUpload({
            requestKey: entry.requestKey,
            imageBlob: entry.file,
            fileName: entry.file.name,
            contributorTag: form.name.trim(),
            pinCode: postal.data,
            description: form.desc,
            cityId: selectedCityId,
            location: chosenLocation,
          });
          if (!isCurrentUserSession(session)) return;
          setFiles((previous) =>
            previous.map((file) =>
              file.requestKey === entry.requestKey
                ? { ...file, status: "saved", error: undefined }
                : file,
            ),
          );
          queued++;
        } catch (error) {
          if (!isCurrentUserSession(session)) return;
          failed++;
          const message =
            error instanceof Error
              ? error.message
              : "Photo could not be saved on this device";
          setFiles((previous) =>
            previous.map((file) =>
              file.requestKey === entry.requestKey
                ? { ...file, status: "error", error: message }
                : file,
            ),
          );
        }
      }
      setLoading(false);
      if (!isCurrentUserSession(session)) return;
      if (queued > 0) {
        addToast(
          `${queued} image${queued > 1 ? "s" : ""} saved offline. Will upload when back online.`,
          "info",
        );
        if (failed === 0) onSubmit();
      }
      if (failed > 0)
        addToast(
          "Some photos could not be saved. Keep this tab open and retry the marked photos.",
          "error",
        );
      return;
    }

    let successCount = 0;
    let errorCount = 0;
    let unresolvedCount = 0;
    let firstError: string | null = null;
    let rateLimited = false;

    for (let i = 0; i < submissions.length; i++) {
      if (!isCurrentUserSession(session)) return;
      if (rateLimited) break;
      if (submissions[i].status === "done") {
        successCount++;
        continue;
      }
      if (submissions[i].status === "saved") continue;

      setFiles((prev) =>
        prev.map((f, idx) =>
          idx === i ? { ...f, status: "uploading", error: undefined } : f,
        ),
      );

      const formData = new FormData();
      formData.append("image", submissions[i].file);
      formData.append("contributor_tag", form.name.trim());
      formData.append("pin_code", postal.data);
      formData.append("description", form.desc);
      formData.append("city_id", selectedCityId);
      if (chosenLocation)
        formData.append("location", JSON.stringify(chosenLocation));

      try {
        const result = await api.upload(formData, submissions[i].requestKey);
        if (!isCurrentUserSession(session)) return;
        if (result.location_source === "unresolved") unresolvedCount++;
        setFiles((prev) =>
          prev.map((f, idx) => (idx === i ? { ...f, status: "done" } : f)),
        );
        successCount++;
      } catch (err) {
        if (!isCurrentUserSession(session)) return;
        const message =
          err instanceof Error
            ? err.message
            : "Upload failed. Check image format and fields.";
        if (message.includes("429") || message.toLowerCase().includes("rate")) {
          rateLimited = true;
        }
        setFiles((prev) =>
          prev.map((f, idx) =>
            idx === i ? { ...f, status: "error", error: message } : f,
          ),
        );
        errorCount++;
        if (!firstError) {
          firstError = message;
        }
      }
    }

    setLoading(false);

    if (rateLimited) {
      setFiles((prev) =>
        prev.map((f) =>
          f.status === "pending"
            ? { ...f, status: "error", error: "Rate limited. Retry later." }
            : f,
        ),
      );
    }

    if (unresolvedCount > 0) {
      addToast(
        `${unresolvedCount} uploaded photo${unresolvedCount > 1 ? "s have" : " has"} no map location. The upload was saved.`,
        "warning",
      );
    }

    if (successCount > 0) {
      addToast(
        `${successCount} photo${successCount > 1 ? "s" : ""} uploaded${errorCount > 0 ? ` (${errorCount} failed)` : ""}`,
        errorCount > 0 ? "warning" : "success",
      );
      if (errorCount > 0 && firstError) {
        addToast(firstError, "error");
      }
      if (errorCount === 0) {
        onSubmit();
      }
    } else {
      addToast(
        firstError || "All uploads failed. Try smaller images.",
        "error",
      );
    }
  };

  const statusIcon = (status: FileStatus) => {
    switch (status) {
      case "uploading":
        return <Loader2 size={16} className="animate-spin text-white" />;
      case "done":
      case "saved":
        return <CheckCircle size={16} className="text-green-400" />;
      case "error":
        return <AlertCircle size={16} className="text-red-400" />;
      default:
        return null;
    }
  };

  return (
    <div className="max-w-5xl mx-auto space-y-12 animate-in pb-32">
      <div className="flex justify-between items-end gap-6 border-b border-line pb-8">
        <div>
          <p className="eyebrow mb-2">Contribute</p>
          <h2 className="display text-4xl sm:text-5xl md:text-6xl">
            Upload lettering
          </h2>
          <p className="handwritten text-lg text-terracotta mt-2">
            Share a photo of street lettering.
          </p>
        </div>
        <button
          onClick={onCancel}
          aria-label="Cancel contribution"
          className="icon-btn shrink-0"
        >
          <X />
        </button>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-12">
        <div className="space-y-6">
          <StepHeading
            n={1}
            done={files.length > 0}
            title="Capture Lettering"
          />

          {/* File grid */}
          {files.length > 0 && (
            <div className="grid grid-cols-3 gap-3">
              {files.map((entry, idx) => (
                <div
                  key={idx}
                  className="relative aspect-square rounded-xl overflow-hidden group bg-line"
                >
                  <img
                    src={entry.preview}
                    className="w-full h-full object-cover"
                    alt={`Preview ${idx + 1}`}
                  />
                  {/* Status overlay */}
                  {entry.status !== "pending" && (
                    <div className="absolute inset-0 bg-black/50 flex items-center justify-center">
                      {statusIcon(entry.status)}
                    </div>
                  )}
                  {entry.status === "error" && entry.error && (
                    <div
                      className="absolute bottom-1 left-1 right-1 bg-black/80 text-white text-xs font-bold px-1 py-0.5 leading-tight"
                      title={entry.error}
                    >
                      {formatErrorLabel(entry.error)}
                    </div>
                  )}
                  {entry.status === "saved" && (
                    <span className="absolute bottom-1 inset-x-1 bg-black/80 text-white text-xs p-1 text-center">
                      Saved for later
                    </span>
                  )}
                  {/* Remove button */}
                  {(entry.status === "pending" || entry.status === "error") && (
                    <button
                      type="button"
                      disabled={loading}
                      aria-label={`Remove ${entry.file.name}`}
                      onClick={() => removeFile(idx)}
                      className="absolute top-1 right-1 bg-surface text-ink border border-line p-1 hover:bg-terracotta hover:text-white transition-colors z-10 shadow-sm"
                      title="Remove image"
                    >
                      <X size={14} />
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}
          {files.some((entry) => entry.status === "error") && (
            <ul
              role="alert"
              className="space-y-2 text-sm text-red-700 break-words"
            >
              {files
                .filter((entry) => entry.status === "error")
                .map((entry) => (
                  <li key={entry.requestKey}>
                    {entry.file.name}: {entry.error}
                  </li>
                ))}
            </ul>
          )}

          {/* Upload area */}
          <button
            type="button"
            disabled={loading}
            aria-label="Choose photos"
            onClick={() => fileRef.current?.click()}
            className="w-full rounded-2xl border-2 border-dashed border-line hover:border-terracotta aspect-[4/3] flex flex-col items-center justify-center bg-surface cursor-pointer overflow-hidden group transition-colors"
          >
            <div className="text-center p-12">
              <Upload
                size={48}
                className="mx-auto mb-4 text-stone-300 group-hover:text-terracotta transition-colors"
              />
              <p className="text-xs font-semibold uppercase tracking-widest text-muted">
                {files.length > 0
                  ? `${files.length} selected — tap to add more`
                  : "Choose photos"}
              </p>
              <p className="text-xs text-muted mt-2 font-bold">
                Multiple images supported
              </p>
            </div>
          </button>

          <input
            type="file"
            ref={fileRef}
            className="hidden"
            accept="image/*"
            multiple
            disabled={loading}
            onChange={(e) => addFiles(e.target.files)}
          />

          {/* Camera button */}
          <button
            type="button"
            disabled={loading}
            onClick={() => cameraRef.current?.click()}
            className="btn-secondary w-full"
          >
            <Camera size={18} />
            Take Photo
          </button>
          <input
            type="file"
            ref={cameraRef}
            className="hidden"
            accept="image/*"
            capture="environment"
            disabled={loading}
            onChange={(e) => addFiles(e.target.files)}
          />
        </div>

        <form
          ref={formRef}
          onSubmit={handleUpload}
          aria-label="Upload lettering"
          aria-busy={loading}
          className="card p-6 md:p-10 shadow-card space-y-8 flex flex-col"
        >
          <StepHeading n={2} done={false} title="Archive Details" />
          {formError && (
            <p
              id={`${id}-error`}
              ref={errorRef}
              tabIndex={-1}
              role="alert"
              className="text-sm text-red-700 break-words"
            >
              {formError}
            </p>
          )}
          <div className="space-y-6 flex-1">
            <div className="space-y-2">
              <label htmlFor={`${id}-name`} className="eyebrow">
                Contributor name
              </label>
              <input
                id={`${id}-name`}
                placeholder="Contributor Name"
                className="field py-3.5 !text-base"
                autoComplete="nickname"
                disabled={loading}
                aria-describedby={formError ? `${id}-error` : undefined}
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                required
              />
            </div>

            <div className="space-y-1 relative">
              <label htmlFor={`${id}-postal`} className="eyebrow">
                Postal / ZIP Code
              </label>
              <input
                id={`${id}-postal`}
                aria-label="Postal or ZIP code"
                aria-describedby={`${id}-postal-help${formError ? ` ${id}-error` : ""}`}
                placeholder="e.g. 560001, SW1A 1AA, 10001"
                className="field py-3.5 !text-base"
                disabled={loading}
                value={form.pin}
                onChange={(e) => setForm({ ...form, pin: e.target.value })}
                required
              />
              <p id={`${id}-postal-help`} className="text-sm text-muted">
                Enter the photo's postal code. It provides an approximate area
                when no point is chosen.
              </p>
            </div>
            <div className="space-y-2">
              <p className="eyebrow">Photo location (optional)</p>
              <p className="text-sm text-muted">
                A confirmed point and place name will be public. It applies to
                the remaining photos in this batch.
              </p>
              {chosenLocation && (
                <p role="status" className="text-sm break-words">
                  {chosenLocation.place_name} —{" "}
                  {chosenLocation.latitude.toFixed(5)},{" "}
                  {chosenLocation.longitude.toFixed(5)}.
                  {chosenLocation.accuracy_m === null
                    ? " Accuracy unknown."
                    : ` Estimated within ${chosenLocation.accuracy_m} metres.`}
                </p>
              )}
              <button
                type="button"
                className="btn-outline"
                disabled={loading}
                onClick={() => setShowLocationPicker(true)}
              >
                <MapPin size={16} />{" "}
                {chosenLocation
                  ? "Change photo location"
                  : "Choose photo location"}
              </button>
              {chosenLocation && (
                <button
                  type="button"
                  className="btn-outline"
                  disabled={loading}
                  onClick={() => setChosenLocation(null)}
                >
                  Remove chosen location
                </button>
              )}
            </div>

            <div className="space-y-2">
              <label htmlFor={`${id}-description`} className="eyebrow">
                Description (optional)
              </label>
              <textarea
                id={`${id}-description`}
                placeholder="Tell the story of this find (material, style, location context)..."
                className="field py-3.5 !text-base"
                disabled={loading}
                rows={5}
                value={form.desc}
                onChange={(e) => setForm({ ...form, desc: e.target.value })}
              />
            </div>
          </div>

          <button
            type="submit"
            disabled={loading || files.length === 0}
            className="btn-primary w-full !py-4 text-base"
          >
            {loading ? (
              <span className="flex items-center justify-center gap-2">
                <Loader2 className="animate-spin" size={20} />
                Uploading...
              </span>
            ) : files.length > 1 ? (
              `Upload ${files.length} photos`
            ) : (
              "Upload photo"
            )}
          </button>
          <button
            type="button"
            className="btn-outline w-full"
            disabled={loading || files.length === 0}
            onClick={(event) => void handleUpload(event, true)}
          >
            Save for later
          </button>
          <Link className="inline-block underline" to="/me/queue">
            Manage saved uploads
          </Link>
        </form>
        {showLocationPicker && (
          <LocationPicker
            value={chosenLocation}
            center={[centerLat, centerLng]}
            onClose={() => setShowLocationPicker(false)}
            onConfirm={(point) => {
              setChosenLocation(point);
              setShowLocationPicker(false);
            }}
          />
        )}
      </div>
    </div>
  );
};

const StepHeading: React.FC<{ n: number; done: boolean; title: string }> = ({
  n,
  done,
  title,
}) => (
  <h4 className="flex items-center gap-3">
    <span
      className={`flex h-8 w-8 items-center justify-center rounded-full text-sm font-semibold transition-colors ${done ? "bg-forest text-white" : "bg-ink text-paper"}`}
    >
      {done ? <CheckCircle size={16} /> : n}
    </span>
    <span>
      <span className="block text-xs font-medium uppercase tracking-wider text-muted">
        Step 0{n}
      </span>
      <span className="block font-display text-lg font-semibold leading-tight">
        {title}
      </span>
    </span>
  </h4>
);

export default ContributionPanel;
