import React, { useState, useRef } from "react";
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

type FileStatus = "pending" | "uploading" | "done" | "error";

interface FileEntry {
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
  const { selectedCityId } = useCityStore();
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [isLocating, setIsLocating] = useState(false);

  const [form, setForm] = useState({
    name: "",
    pin: "",
    desc: "",
  });
  const fileRef = useRef<HTMLInputElement>(null);
  const cameraRef = useRef<HTMLInputElement>(null);

  const detectLocation = () => {
    if (!navigator.geolocation) {
      addToast("Geolocation not supported by this browser", "info");
      return;
    }
    
    setIsLocating(true);
    navigator.geolocation.getCurrentPosition(
      async (pos) => {
        try {
          const res = await fetch(
            `https://nominatim.openstreetmap.org/reverse?format=json&lat=${pos.coords.latitude}&lon=${pos.coords.longitude}`
          );
          const data = await res.json();
          const pc = data.address?.postcode?.trim();

          if (pc) {
            setForm((prev) => ({ ...prev, pin: pc }));
            addToast(`Detected postal code: ${pc}`, "success");
          } else {
            addToast("Could not determine PIN for this spot", "info");
          }
        } catch {
          addToast("Location service busy. Please enter PIN manually.", "info");
        } finally {
          setIsLocating(false);
        }
      },
      (err) => {
        setIsLocating(false);
        if (err.code === 1) {
          addToast("Location access denied. Please enter the photo's PIN manually.", "info");
        } else {
          addToast("GPS Signal weak. Enter PIN manually.", "info");
        }
      },
      { timeout: 10000 }
    );
  };

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

  const handleUpload = async (e: React.FormEvent) => {
    e.preventDefault();
    if (files.length === 0)
      return addToast("At least one image required", "error");
    if (!form.name.trim())
      return addToast("Contributor name required", "error");
    if (!selectedCityId)
      return addToast("Please select a city before uploading", "error");
    setLoading(true);

    // Offline mode: queue all to IndexedDB
    if (!navigator.onLine) {
      let queued = 0;
      for (const entry of files) {
        try {
          await enqueueUpload({
            imageBlob: entry.file,
            fileName: entry.file.name,
            contributorTag: form.name.trim(),
            pinCode: form.pin,
            description: form.desc,
            cityId: selectedCityId,
          });
          queued++;
        } catch {
          // IndexedDB error
        }
      }
      setLoading(false);
      if (queued > 0) {
        addToast(
          `${queued} image${queued > 1 ? "s" : ""} saved offline. Will upload when back online.`,
          "info",
        );
        onSubmit();
      } else {
        addToast("Failed to save offline", "error");
      }
      return;
    }

    let successCount = 0;
    let errorCount = 0;
    let firstError: string | null = null;
    let rateLimited = false;

    for (let i = 0; i < files.length; i++) {
      if (rateLimited) break;
      if (files[i].status === "done") {
        successCount++;
        continue;
      }

      setFiles((prev) =>
        prev.map((f, idx) =>
          idx === i ? { ...f, status: "uploading", error: undefined } : f,
        ),
      );

      const formData = new FormData();
      formData.append("image", files[i].file);
      formData.append("contributor_tag", form.name.trim());
      formData.append("pin_code", form.pin);
      formData.append("description", form.desc);
      formData.append("city_id", selectedCityId);

      try {
        await api.upload(formData);
        setFiles((prev) =>
          prev.map((f, idx) => (idx === i ? { ...f, status: "done" } : f)),
        );
        successCount++;
      } catch (err) {
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

    if (successCount > 0) {
      addToast(
        `${successCount} artifact${successCount > 1 ? "s" : ""} submitted${errorCount > 0 ? ` (${errorCount} failed)` : ""}`,
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
            Contributor Lab
          </h2>
          <p className="handwritten text-lg text-terracotta mt-2">
            Preserving the city's lettered soul...
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
          <StepHeading n={1} done={files.length > 0} title="Capture Lettering" />

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
                  {/* Remove button */}
                  {entry.status === "pending" && (
                    <button
                      onClick={() => removeFile(idx)}
                      className="absolute top-1 right-1 bg-black/80 text-white p-0.5 opacity-0 group-hover:opacity-100 transition-opacity"
                    >
                      <X size={14} />
                    </button>
                  )}
                  {(entry.status === "pending" || entry.status === "error") && (
                    <button
                      type="button" // Important to prevent form submission
                      onClick={(e) => {
                        e.stopPropagation(); // Prevent triggering any parent clicks
                        removeFile(idx);
                      }}
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

          {/* Upload area */}
          <div
            onClick={() => fileRef.current?.click()}
            className="rounded-2xl border-2 border-dashed border-line hover:border-terracotta aspect-[4/3] flex flex-col items-center justify-center bg-surface cursor-pointer overflow-hidden group transition-colors"
          >
            <div className="text-center p-12">
              <Upload
                size={48}
                className="mx-auto mb-4 text-stone-300 group-hover:text-terracotta transition-colors"
              />
              <p className="text-xs font-semibold uppercase tracking-widest text-stone-400">
                {files.length > 0
                  ? `${files.length} selected — tap to add more`
                  : "Tap to mount specimens"}
              </p>
              <p className="text-xs text-stone-300 mt-2 font-bold">
                Multiple images supported
              </p>
            </div>
          </div>

          <input
            type="file"
            ref={fileRef}
            className="hidden"
            accept="image/*"
            multiple
            onChange={(e) => addFiles(e.target.files)}
          />

          {/* Camera button */}
          <button
            type="button"
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
            onChange={(e) => addFiles(e.target.files)}
          />
        </div>

        <form
          onSubmit={handleUpload}
          className="card p-6 md:p-10 shadow-card space-y-8 flex flex-col"
        >
          <StepHeading n={2} done={false} title="Archive Details" />
          <div className="space-y-6 flex-1">
            <input
              placeholder="Contributor Name"
              className="field py-3.5"
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              required
            />

            <div className="space-y-1 relative">
              <label className="eyebrow">
                Postal / ZIP Code
              </label>
              <input
                placeholder="e.g. 560001, SW1A 1AA, 10001"
                className="field py-3.5 pr-10"
                value={form.pin}
                onChange={(e) => setForm({ ...form, pin: e.target.value })}
                required
              />
              <button
                type="button"
                onClick={detectLocation}
                className="absolute right-3 top-10 text-terracotta"
              >
                {isLocating ? (
                  <Loader2 size={18} className="animate-spin" />
                ) : (
                  <MapPin size={18} />
                )}
              </button>
            </div>

            <textarea
              placeholder="Tell the story of this find (material, style, location context)..."
              className="field py-3.5"
              rows={5}
              value={form.desc}
              onChange={(e) => setForm({ ...form, desc: e.target.value })}
            />
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
              `Archive ${files.length} Specimens`
            ) : (
              "Finalize Archiving"
            )}
          </button>
        </form>
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
