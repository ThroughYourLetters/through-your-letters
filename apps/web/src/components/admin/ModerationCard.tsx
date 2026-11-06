import {
  AlertTriangle,
  Check,
  ExternalLink,
  Heart,
  MapPin,
  MessageCircle,
  RefreshCw,
  Trash2,
  Users,
  X,
} from "lucide-react";
import type { ModerationItem } from "@ttl/types";
const ModerationCard = ({
  item,
  onApprove,
  onReject,
  onDelete,
  isProcessing,
  isReported,
  selected,
  onToggleSelected,
}: {
  item: ModerationItem;
  onApprove: () => void;
  onReject?: () => void;
  onDelete: () => void;
  isProcessing: boolean;
  isReported?: boolean;
  selected: boolean;
  onToggleSelected: () => void;
}) => (
  <div className="card p-4 md:p-6 flex flex-col md:flex-row gap-8 transition-all hover:bg-stone-50">
    <div className="w-full md:w-56 h-56 flex-shrink-0 border border-line bg-stone-100 overflow-hidden relative group">
      <label className="absolute top-2 left-2 z-10 bg-white/90 border border-line px-1 py-0.5">
        <input
          type="checkbox"
          aria-label={`Select upload ${item.id}`}
          checked={selected}
          disabled={isProcessing}
          onChange={onToggleSelected}
        />
      </label>
      <img
        src={item.image_url}
        className="w-full h-full object-cover transition-transform group-hover:scale-105"
        alt="Uploaded lettering"
      />
      <a
        href={item.image_url}
        aria-label={`Open photo ${item.id}`}
        target="_blank"
        rel="noreferrer"
        className="absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 flex items-center justify-center transition-opacity"
      >
        <ExternalLink className="text-white" size={24} />
      </a>
    </div>
    <div className="flex-1 min-w-0 space-y-6">
      <div className="flex justify-between items-start gap-4">
        <div className="space-y-1">
          <p className="text-xs font-semibold uppercase text-terracotta flex items-center gap-2">
            <MapPin size={10} /> {item.pin_code} // <Users size={10} />{" "}
            {item.contributor_tag}
          </p>
          <h3 className="font-display font-semibold tracking-tight text-2xl break-words">
            {item.detected_text || "No recognized text"}
          </h3>
          <p className="text-xs font-bold text-stone-400 uppercase">
            {new Date(item.created_at).toLocaleString()}
          </p>
        </div>
        {isReported && (
          <div className="bg-red-50 border border-red-600 px-4 py-2 flex items-center gap-2 text-red-700 font-semibold text-xs uppercase">
            <AlertTriangle size={14} /> {item.report_count} Flags
          </div>
        )}
      </div>

      <div className="grid grid-cols-2 gap-6">
        <div className="space-y-2">
          <p className="text-xs font-semibold uppercase text-stone-400 tracking-widest">
            Description
          </p>
          <p className="text-sm font-medium text-stone-700 leading-relaxed italic break-words line-clamp-3">
            "{item.description || "No context provided."}"
          </p>
        </div>
        <div className="space-y-2 border-l border-stone-100 pl-6">
          <p className="text-xs font-semibold uppercase text-stone-400 tracking-widest">
            Signals
          </p>
          <div className="flex flex-wrap gap-3">
            <span className="flex items-center gap-1 text-xs font-semibold">
              <Heart size={12} /> {item.likes_count} likes
            </span>
            <span className="flex items-center gap-1 text-xs font-semibold">
              <MessageCircle size={12} /> {item.comments_count} comments
            </span>
          </div>
        </div>
      </div>

      {item.cultural_context && (
        <div className="bg-stone-50 p-4 border-l border-forest space-y-1">
          <p className="text-xs font-semibold uppercase text-forest tracking-widest">
            Location context
          </p>
          <p className="text-sm text-stone-700 leading-relaxed italic line-clamp-4">
            {item.cultural_context}
          </p>
        </div>
      )}

      {isReported && item.report_reasons && (
        <div className="bg-red-50/50 p-4 border-l border-red-600 space-y-1">
          <p className="text-xs font-semibold uppercase text-red-600 tracking-widest">
            User Complaints:
          </p>
          {item.report_reasons.map((r: string, i: number) => (
            <p key={i} className="text-sm font-bold text-red-900">
              - {r}
            </p>
          ))}
        </div>
      )}

      <div className="flex flex-wrap gap-3 pt-2">
        <button
          disabled={isProcessing}
          onClick={onApprove}
          className="flex-1 bg-ink text-white py-4 font-semibold uppercase text-xs tracking-widest flex items-center justify-center gap-2 hover:bg-green-600 transition-all disabled:opacity-50"
        >
          {isProcessing ? (
            <RefreshCw className="animate-spin" size={16} />
          ) : (
            <Check size={18} />
          )}
          {isReported ? "Clear Flags" : "Approve"}
        </button>
        {!isReported && (
          <button
            disabled={isProcessing}
            onClick={onReject}
            className="flex-1 border border-line py-4 font-semibold uppercase text-xs flex items-center justify-center gap-2 hover:bg-red-50 disabled:opacity-50 transition-all"
          >
            <X size={18} /> Reject
          </button>
        )}
        <button
          disabled={isProcessing}
          onClick={onDelete}
          aria-label={`Delete upload ${item.id}`}
          className="px-4 border border-line py-4 font-semibold uppercase text-xs text-red-600 hover:bg-red-600 hover:text-white disabled:opacity-50 transition-all"
        >
          <Trash2 size={18} />
        </button>
      </div>
    </div>
  </div>
);
export default ModerationCard;
