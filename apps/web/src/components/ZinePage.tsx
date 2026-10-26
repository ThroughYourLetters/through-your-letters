import React, { useEffect, useState } from "react";
import { ZinePageData, Comment, RevisitLink } from "../types";
import {
  MapPin,
  Share2,
  Trash2,
  AlertTriangle,
  AlignLeft,
  Heart,
  MessageCircle,
  Send,
  ChevronDown,
  ChevronUp,
  Loader2,
  Download,
} from "lucide-react";
import { useToastStore } from "../store/useToastStore";
import { useAuthStore } from "../store/useAuthStore";
import { API_BASE_URL } from "../constants";
import { api } from "../lib/api";
import BeforeAfterSlider from "./BeforeAfterSlider";

const ZinePage: React.FC<{
  page: ZinePageData;
  onDelete?: (id: string | number) => void;
  onImageClick?: () => void;
  onContributorClick?: () => void;
}> = ({ page, onDelete, onImageClick, onContributorClick }) => {
  const { addToast } = useToastStore();
  const { user, hydrated, hydrate } = useAuthStore();

  // Like state
  const [liked, setLiked] = useState(false);
  const [likesCount, setLikesCount] = useState(page.likes_count || 0);
  const [likeLoading, setLikeLoading] = useState(false);

  // Comments state
  const [comments, setComments] = useState<Comment[]>([]);
  const [showComments, setShowComments] = useState(false);
  const [newComment, setNewComment] = useState("");
  const [commentsLoading, setCommentsLoading] = useState(false);
  const [commentSubmitting, setCommentSubmitting] = useState(false);
  const [commentsLoaded, setCommentsLoaded] = useState(false);
  const [revisits, setRevisits] = useState<RevisitLink[]>([]);
  const [similar, setSimilar] = useState<
    Array<{
      id: string;
      thumbnail?: string;
      image_url: string;
      detected_text?: string;
      ml_style?: string;
      ml_script?: string;
    }>
  >([]);

  const handleShare = async () => {
    const url = `${window.location.origin}/#page-${page.id}`;

    // Try sharing with image file first
    try {
      const imageRes = await fetch(page.image);
      const blob = await imageRes.blob();
      const ext = blob.type.includes("webp") ? "webp" : "jpg";
      const file = new File([blob], `tyl-${page.id}.${ext}`, {
        type: blob.type,
      });
      const shareWithFile = {
        title: `Through Your Letters: ${page.title}`,
        text: `Check out this typography artifact from ${page.location}`,
        url,
        files: [file],
      };
      if (navigator.canShare?.(shareWithFile)) {
        await navigator.share(shareWithFile);
        return;
      }
    } catch {
      // Image share not supported or failed, fall through
    }

    // Fallback: share without file
    const shareData = {
      title: `Through Your Letters: ${page.title}`,
      text: `Check out this typography artifact from ${page.location}`,
      url,
    };

    try {
      if (navigator.share && navigator.canShare?.(shareData)) {
        await navigator.share(shareData);
      } else {
        await navigator.clipboard.writeText(url);
        addToast("Link copied to clipboard", "success");
      }
    } catch (err) {
      if ((err as Error).name !== "AbortError")
        addToast("Share failed", "error");
    }
  };

  const handleReport = () => {
    const reason = window.prompt("Why are you reporting this image?");
    if (!reason) return;
    api
      .reportLettering(page.id, reason)
      .then(() => addToast("Report submitted for review", "success"))
      .catch(() => addToast("Failed to submit report", "error"));
  };

  const handleLike = async () => {
    if (likeLoading) return;
    setLikeLoading(true);
    try {
      const data = await api.toggleLike(page.id);
      setLiked(data.liked);
      setLikesCount(data.likes_count);
    } catch {
      addToast("Failed to toggle like", "error");
    } finally {
      setLikeLoading(false);
    }
  };

  const fetchComments = async () => {
    setCommentsLoading(true);
    try {
      const data = await api.getComments(page.id);
      setComments(data);
      setCommentsLoaded(true);
    } catch {
      addToast("Failed to load comments", "error");
    } finally {
      setCommentsLoading(false);
    }
  };

  const toggleComments = () => {
    if (!showComments && !commentsLoaded) {
      fetchComments();
    }
    setShowComments(!showComments);
  };

  const handleAddComment = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!user) {
      addToast("Sign in to post comments", "warning");
      return;
    }
    if (!newComment.trim()) return;
    setCommentSubmitting(true);
    try {
      const comment = await api.addComment(page.id, newComment.trim());
      if (comment.status === "VISIBLE" || !comment.status) {
        setComments((prev) => [comment, ...prev]);
      } else {
        addToast("Comment submitted and held for moderator review.", "info");
      }
      setNewComment("");
    } catch {
      addToast("Failed to add comment", "error");
    } finally {
      setCommentSubmitting(false);
    }
  };

  const narrative = page.description || page.culturalContext;

  useEffect(() => {
    if (!hydrated) {
      hydrate();
    }
  }, [hydrated, hydrate]);

  useEffect(() => {
    let active = true;
    api
      .getRevisits(page.id)
      .then((data) => {
        if (active) setRevisits(data.revisits || []);
      })
      .catch(() => {
        if (active) setRevisits([]);
      });
    return () => {
      active = false;
    };
  }, [page.id]);

  useEffect(() => {
    let active = true;
    api
      .getSimilar(page.id)
      .then((data) => {
        if (active) setSimilar(data.similar || []);
      })
      .catch(() => {
        if (active) setSimilar([]);
      });
    return () => {
      active = false;
    };
  }, [page.id]);

  return (
    <article
      id={`page-${page.id}`}
      className="grid gap-8 md:grid-cols-12 md:gap-12 items-start scroll-mt-40 py-12 border-b border-line last:border-b-0"
    >
      <figure className="md:col-span-7 group">
        <div className="overflow-hidden rounded-2xl bg-line">
          <img
            src={page.image}
            className={`w-full aspect-[4/3] object-cover transition-transform duration-700 group-hover:scale-[1.02] ${onImageClick ? "cursor-zoom-in" : ""}`}
            alt={page.title}
            onClick={onImageClick}
          />
        </div>
        <figcaption className="mt-3 flex items-center justify-between gap-4 text-sm text-muted">
          <span className="flex items-center gap-1.5">
            <MapPin size={14} className="text-terracotta" />
            {page.location}
          </span>
          {onContributorClick ? (
            <button
              onClick={onContributorClick}
              className="font-medium text-ink hover:text-terracotta transition-colors"
            >
              By {page.contributorName}
            </button>
          ) : (
            <span>By {page.contributorName}</span>
          )}
        </figcaption>
      </figure>

      <div className="md:col-span-5 flex flex-col gap-6">
        <div className="flex flex-wrap items-center gap-2">
          <span className="chip border-terracotta/30 bg-terracotta-soft text-terracotta-dark">
            {page.vibe || "Unknown Style"}
          </span>
          {page.ml_script && <span className="chip">{page.ml_script}</span>}
        </div>

        <h2 className="display text-4xl md:text-5xl leading-[1.02] break-words">
          {page.title}
        </h2>

        <div className="space-y-2">
          <h4 className="eyebrow flex items-center gap-2 text-terracotta">
            <AlignLeft size={13} />
            Museum Context & Story
          </h4>
          <p className="text-lg leading-relaxed text-ink/90 break-words whitespace-pre-wrap">
            {narrative}
          </p>
        </div>

        <div className="rounded-xl border border-line bg-paper px-5 py-4">
          <p className="eyebrow mb-1">Archival Record</p>
          <p className="font-display italic text-ink/80 break-words">
            {page.historicalNote}
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <button
            onClick={handleLike}
            disabled={likeLoading}
            className={`icon-btn px-3 ${liked ? "!border-terracotta text-terracotta" : ""}`}
            title="Like"
          >
            <Heart size={15} fill={liked ? "currentColor" : "none"} />
            <span className="text-xs font-medium">{likesCount}</span>
          </button>
          <button onClick={toggleComments} className="icon-btn px-3" title="Comments">
            <MessageCircle size={15} />
            <span className="text-xs font-medium">{page.comments_count || 0}</span>
          </button>
          <button onClick={handleShare} className="icon-btn" title="Share" aria-label="Share">
            <Share2 size={15} />
          </button>
          <a
            href={`${API_BASE_URL}/api/v1/letterings/${page.id}/download`}
            target="_blank"
            rel="noreferrer"
            className="icon-btn"
            title="Download"
            aria-label="Download"
          >
            <Download size={15} />
          </a>
          <button
            onClick={handleReport}
            className="icon-btn hover:!border-ochre hover:!text-ochre"
            title="Report"
            aria-label="Report"
          >
            <AlertTriangle size={15} />
          </button>
          {onDelete && page.is_owner && (
            <button
              onClick={() => onDelete(page.id)}
              className="icon-btn text-red-600 hover:!border-red-600 hover:!text-red-700"
              title="Delete"
              aria-label="Delete"
            >
              <Trash2 size={15} />
            </button>
          )}
        </div>

        {showComments ? (
          <div className="card overflow-hidden">
            <button
              onClick={toggleComments}
              className="w-full flex items-center justify-between px-5 py-3 border-b border-line hover:bg-paper rounded-none"
            >
              <span className="eyebrow">Comments</span>
              <ChevronUp size={16} className="text-muted" />
            </button>

            <form onSubmit={handleAddComment} className="p-4 flex items-center gap-2">
              <input
                type="text"
                value={newComment}
                onChange={(e) => setNewComment(e.target.value)}
                placeholder={user ? "Add a comment..." : "Sign in to comment"}
                className="field rounded-full"
                disabled={commentSubmitting || !user}
                maxLength={500}
              />
              <button
                type="submit"
                disabled={commentSubmitting || !newComment.trim() || !user}
                className="btn-dark !px-3 !py-2.5"
                aria-label="Post comment"
              >
                {commentSubmitting ? (
                  <Loader2 size={16} className="animate-spin" />
                ) : (
                  <Send size={16} />
                )}
              </button>
            </form>
            {!user && (
              <p className="px-5 -mt-2 pb-2 text-xs text-muted">
                Sign in from the account page to comment.
              </p>
            )}

            <div className="px-5 pb-5 space-y-4 max-h-64 overflow-y-auto">
              {commentsLoading ? (
                <div className="flex justify-center py-4">
                  <Loader2 size={20} className="animate-spin text-terracotta" />
                </div>
              ) : comments.length === 0 ? (
                <p className="text-sm text-muted text-center py-4">
                  No comments yet. Be the first.
                </p>
              ) : (
                comments.map((comment) => (
                  <div key={comment.id} className="border-l-2 border-terracotta/30 pl-3 space-y-0.5">
                    <p className="text-sm">
                      <span className="font-semibold">
                        {comment.commenter_name || "Anonymous"}
                      </span>{" "}
                      <span className="text-xs text-muted">
                        · {new Date(comment.created_at).toLocaleDateString()}
                      </span>
                    </p>
                    <p className="text-sm text-ink/90">{comment.content}</p>
                  </div>
                ))
              )}
            </div>
          </div>
        ) : (
          <button
            onClick={toggleComments}
            className="self-start flex items-center gap-1.5 text-sm text-muted hover:text-ink transition-colors"
          >
            <ChevronDown size={14} />
            Show Comments ({page.comments_count || 0})
          </button>
        )}

        {revisits.length > 0 && (
          <div className="space-y-3">
            <h4 className="eyebrow text-terracotta">Before / After</h4>
            {revisits.map((revisit) => (
              <BeforeAfterSlider key={revisit.id} revisit={revisit} />
            ))}
          </div>
        )}

        {similar.length > 0 && (
          <div className="space-y-3">
            <h4 className="eyebrow">Similar Lettering</h4>
            <div className="flex gap-3 overflow-x-auto pb-2">
              {similar.map((item) => (
                <a
                  key={item.id}
                  href={`#page-${item.id}`}
                  className="shrink-0 overflow-hidden rounded-xl bg-line hover:opacity-80 transition-opacity"
                >
                  <img
                    src={item.thumbnail || item.image_url}
                    alt={item.detected_text || "Similar lettering"}
                    className="w-20 h-20 object-cover"
                  />
                </a>
              ))}
            </div>
          </div>
        )}
      </div>
    </article>
  );
};

export default ZinePage;
