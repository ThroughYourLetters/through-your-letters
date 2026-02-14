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
import { AREA_PIN_MAP, PIN_AREA_MAP } from "../constants";
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
    area: "Other",
    pin: "",
    desc: "",
  });
  const fileRef = useRef<HTMLInputElement>(null);
  const cameraRef = useRef<HTMLInputElement>(null);

  const handlePinChange = (val: string) => {
    const pin = val.replace(/\D/g, "").substring(0, 6);
    const matchedArea = PIN_AREA_MAP[pin] || "Other";
    setForm((prev) => ({ ...prev, pin, area: matchedArea }));
  };

  const handleAreaChange = (val: string) => {
    const matchedPin = AREA_PIN_MAP[val] || "";
    setForm((prev) => ({ ...prev, area: val, pin: matchedPin || prev.pin }));
  };

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
          const pc = data.address.postcode?.replace(/\s/g, "").substring(0, 6);
          
          if (pc) {
            handlePinChange(pc);
            addToast(`Detected PIN: ${pc}`, "success");
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
            cityId: selectedCityId || "0194f123-4567-7abc-8def-0123456789ab",
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
      formData.append(
        "city_id",
        selectedCityId || "0194f123-4567-7abc-8def-0123456789ab",
      );

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
      <div className="flex justify-between items-center bg-black text-white p-6 brutalist-shadow">
        <div>
          <h2 className="text-3xl font-black uppercase tracking-tighter">
            Contributor Lab
          </h2>
          <p className="handwritten text-sm text-[#d4a017] italic">
            Preserving the city's lettered soul...
          </p>
        </div>
        <button
          onClick={onCancel}
          className="p-2 bg-white text-black border-2 border-black hover:bg-[#cc543a] hover:text-white transition-colors"
        >
          <X />
        </button>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-12">
        <div className="space-y-6">
          <h4 className="text-xs font-black uppercase bg-[#2d5a27] text-white px-2 py-1 inline-block">
            Step 01: Capture Lettering
          </h4>

          {/* File grid */}
          {files.length > 0 && (
            <div className="grid grid-cols-3 gap-3">
              {files.map((entry, idx) => (
                <div
                  key={idx}
                  className="relative aspect-square border-2 border-black overflow-hidden group"
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
                      className="absolute bottom-1 left-1 right-1 bg-black/80 text-white text-[7px] font-bold px-1 py-0.5 leading-tight"
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
                      className="absolute top-1 right-1 bg-white text-black border border-black p-1 hover:bg-[#cc543a] hover:text-white transition-colors z-10 shadow-sm"
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
            className="border-4 border-black aspect-[4/3] flex flex-col items-center justify-center bg-white brutalist-shadow-sm cursor-pointer overflow-hidden group"
          >
            <div className="text-center p-12">
              <Upload
                size={48}
                className="mx-auto mb-4 text-slate-300 group-hover:text-black transition-colors"
              />
              <p className="text-[10px] font-black uppercase tracking-widest text-slate-400">
                {files.length > 0
                  ? `${files.length} selected — tap to add more`
                  : "Tap to mount specimens"}
              </p>
              <p className="text-[9px] text-slate-300 mt-2 font-bold">
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
            className="w-full flex items-center justify-center gap-2 border-2 border-black py-3 font-black text-[10px] uppercase hover:bg-black hover:text-white transition-all"
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
          className="bg-white p-8 md:p-10 border-4 border-black brutalist-shadow space-y-8 flex flex-col"
        >
          <h4 className="text-[10px] font-black uppercase text-[#cc543a]">
            Step 02: Archive Details
          </h4>
          <div className="space-y-6 flex-1">
            <input
              placeholder="Contributor Name"
              className="w-full border-2 border-black p-4 font-black text-sm focus:border-[#cc543a] outline-none"
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              required
            />

            <div className="grid grid-cols-2 gap-4 items-end">
              <div className="space-y-1">
                <label className="text-[8px] font-black uppercase text-slate-400">
                  Neighborhood
                </label>
                <select
                  className="w-full border-2 border-black p-4 font-black bg-white text-sm outline-none"
                  value={form.area}
                  onChange={(e) => handleAreaChange(e.target.value)}
                >
                  <option value="Other">Other Area</option>
                  {Object.keys(AREA_PIN_MAP).map((a) => (
                    <option key={a} value={a}>
                      {a}
                    </option>
                  ))}
                </select>
              </div>
              <div className="space-y-1 relative">
                <label className="text-[8px] font-black uppercase text-slate-400">
                  PIN Code
                </label>
                <input
                  placeholder="560xxx"
                  className="w-full border-2 border-black p-4 font-black text-sm outline-none pr-10"
                  value={form.pin}
                  onChange={(e) => handlePinChange(e.target.value)}
                  required
                />
                <button
                  type="button"
                  onClick={detectLocation}
                  className="absolute right-3 top-10 text-[#cc543a]"
                >
                  {isLocating ? (
                    <Loader2 size={18} className="animate-spin" />
                  ) : (
                    <MapPin size={18} />
                  )}
                </button>
              </div>
            </div>

            <textarea
              placeholder="Tell the story of this find (material, style, location context)..."
              className="w-full border-2 border-black p-4 font-medium text-sm focus:border-[#cc543a] outline-none"
              rows={5}
              value={form.desc}
              onChange={(e) => setForm({ ...form, desc: e.target.value })}
            />
          </div>

          <button
            type="submit"
            disabled={loading || files.length === 0}
            className="w-full bg-black text-white py-6 font-black uppercase brutalist-shadow hover:bg-[#cc543a] transition-all disabled:opacity-50"
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

export default ContributionPanel;
