import { ExternalLink, Eye, EyeOff, RefreshCw, Trash2 } from "lucide-react";
import type { AdminCommentItem } from "@ttl/types";
const CommentModerationCard = ({
  item,
  isProcessing,
  selected,
  onToggleSelected,
  onHide,
  onRestore,
  onDelete,
}: {
  item: AdminCommentItem;
  isProcessing: boolean;
  selected: boolean;
  onToggleSelected: () => void;
  onHide: () => void;
  onRestore: () => void;
  onDelete: () => void;
}) => (
  <div className="card p-5 md:p-6 space-y-4">
    <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-3">
      <div className="space-y-1">
        <p className="text-xs font-semibold uppercase text-terracotta tracking-widest">
          {item.commenter_name || "Anonymous"}{" "}
          {item.commenter_email ? `(${item.commenter_email})` : ""}
        </p>
        <p className="text-sm font-bold text-stone-900 break-words">
          {item.content}
        </p>
        <p className="text-xs font-bold uppercase text-stone-400">
          {new Date(item.created_at).toLocaleString()}
        </p>
      </div>
      <div className="flex items-center gap-2">
        <label className="border border-line px-2 py-1 bg-surface">
          <input
            type="checkbox"
            aria-label={`Select comment ${item.id}`}
            disabled={isProcessing}
            checked={selected}
            onChange={onToggleSelected}
          />
        </label>
        <span
          className={`px-2 py-1 text-xs font-semibold uppercase border border-line ${item.status === "HIDDEN" ? "bg-red-50 text-red-700" : "bg-green-50 text-green-700"}`}
        >
          {item.status}
        </span>
      </div>
    </div>

    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      <div className="space-y-1">
        <p className="text-xs font-semibold uppercase text-stone-400 tracking-widest">
          Upload details
        </p>
        <p className="text-xs font-semibold uppercase">
          PIN {item.pin_code} // {item.contributor_tag}
        </p>
        <a
          href={item.lettering_image_url}
          target="_blank"
          rel="noreferrer"
          className="inline-flex items-center gap-1 text-xs font-semibold uppercase text-terracotta hover:underline"
        >
          <ExternalLink size={12} />
          View upload
        </a>
      </div>
      <div className="space-y-1">
        <p className="text-xs font-semibold uppercase text-stone-400 tracking-widest">
          Moderation
        </p>
        <p className="text-xs font-semibold uppercase">
          Score {item.moderation_score} / Priority {item.review_priority}
        </p>
        <p className="text-xs font-semibold uppercase text-stone-500">
          {item.auto_flagged
            ? "AUTO FLAGGED"
            : item.needs_review
              ? "REVIEW NEEDED"
              : "NO ACTIVE RISK SIGNAL"}
        </p>
        {item.moderation_flags?.length > 0 && (
          <div className="flex flex-wrap gap-2 py-1">
            {item.moderation_flags.map((flag) => (
              <span
                key={flag}
                className="border border-line px-1.5 py-0.5 text-xs font-semibold uppercase bg-yellow-50"
              >
                {flag}
              </span>
            ))}
          </div>
        )}
        {item.moderation_reason ? (
          <p className="text-sm font-medium text-stone-700 italic break-words">
            "{item.moderation_reason}"
          </p>
        ) : (
          <p className="text-sm font-medium text-stone-400 italic">
            No moderation reason recorded
          </p>
        )}
        {item.moderated_by && (
          <p className="text-xs font-bold uppercase text-stone-400">
            by {item.moderated_by}
          </p>
        )}
      </div>
    </div>

    <div className="flex gap-3 pt-2">
      {item.status === "VISIBLE" ? (
        <button
          disabled={isProcessing}
          onClick={onHide}
          className="flex-1 border border-line py-3 font-semibold uppercase text-xs flex items-center justify-center gap-2 hover:bg-yellow-50 disabled:opacity-50 transition-all"
        >
          {isProcessing ? (
            <RefreshCw size={16} className="animate-spin" />
          ) : (
            <EyeOff size={16} />
          )}
          Hide
        </button>
      ) : (
        <button
          disabled={isProcessing}
          onClick={onRestore}
          className="flex-1 border border-line py-3 font-semibold uppercase text-xs flex items-center justify-center gap-2 hover:bg-green-50 disabled:opacity-50 transition-all"
        >
          {isProcessing ? (
            <RefreshCw size={16} className="animate-spin" />
          ) : (
            <Eye size={16} />
          )}
          Restore
        </button>
      )}
      <button
        disabled={isProcessing}
        onClick={onDelete}
        aria-label={`Delete comment ${item.id}`}
        className="px-8 border border-line py-3 font-semibold uppercase text-xs text-red-600 hover:bg-red-600 hover:text-white disabled:opacity-50 transition-all"
      >
        <Trash2 size={18} />
      </button>
    </div>
  </div>
);
export default CommentModerationCard;
