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
    <div
      id={`page-${page.id}`}
      className="flex flex-col md:flex-row gap-12 items-start scroll-mt-24 pb-24 border-b-2 border-black/10 last:border-b-0 overflow-hidden"
    >
      <div className="w-full md:w-3/5 relative py-6 group">
        <div className="tape absolute top-2 left-1/4 w-20 h-8 -rotate-12 opacity-70"></div>
        <div className="tape absolute -bottom-2 right-1/4 w-16 h-8 rotate-6 opacity-70"></div>

        <div className="p-3 bg-white border-2 border-black brutalist-shadow transition-all duration-500 hover:rotate-1">
          <img
            src={page.image}
            className={`w-full aspect-square object-cover contrast-125 grayscale hover:grayscale-0 transition-all duration-700 ${onImageClick ? "cursor-zoom-in" : ""}`}
            alt={page.title}
            onClick={onImageClick}
          />
          <div className="p-4 flex justify-between items-center border-t border-black/5 mt-2 bg-slate-50/50">
            <div className="flex items-center gap-2">
              <MapPin size={14} className="text-[#cc543a]" />
              <span className="text-[10px] font-black uppercase tracking-widest">
                {page.location}
              </span>
            </div>
            {onContributorClick ? (
              <button
                onClick={onContributorClick}
                className="text-[9px] font-black uppercase text-[#cc543a] hover:underline"
              >
                By {page.contributorName}
              </button>
            ) : (
              <span className="text-[9px] font-black uppercase text-slate-500">
                By {page.contributorName}
              </span>
            )}
          </div>
        </div>
      </div>

      <div className="w-full md:w-2/5 flex flex-col space-y-8">
        <div className="flex justify-between items-start">
          <div className="bg-black text-white px-4 py-1.5 text-xs font-black uppercase rotate-1 shadow-[4px_4px_0_0_#cc543a]">
            {page.vibe}
          </div>
          <div className="flex gap-2">
            <button
              onClick={handleLike}
              disabled={likeLoading}
              className={`p-2 border-2 border-black bg-white hover:bg-red-50 transition-colors flex items-center gap-1 ${liked ? "text-[#cc543a]" : "text-slate-400"}`}
              title="Like"
            >
              <Heart size={16} fill={liked ? "currentColor" : "none"} />
              <span className="text-[10px] font-black">{likesCount}</span>
            </button>
            <button
              onClick={toggleComments}
              className="p-2 border-2 border-black bg-white hover:bg-slate-100 flex items-center gap-1 text-slate-600"
              title="Comments"
            >
              <MessageCircle size={16} />
              <span className="text-[10px] font-black">
                {page.comments_count || 0}
              </span>
            </button>
            <button
              onClick={handleShare}
              className="p-2 border-2 border-black bg-white hover:bg-slate-100"
              title="Share"
            >
              <Share2 size={16} />
            </button>
            <a
              href={`${API_BASE_URL}/api/v1/letterings/${page.id}/download`}
              target="_blank"
              rel="noreferrer"
              className="p-2 border-2 border-black bg-white hover:bg-slate-100"
              title="Download"
            >
              <Download size={16} />
            </a>
            <button
              onClick={handleReport}
              className="p-2 border-2 border-black bg-white hover:bg-yellow-50 text-yellow-700"
              title="Report"
            >
              <AlertTriangle size={16} />
            </button>
            {onDelete && page.is_owner && (
              <button
                onClick={() => onDelete(page.id)}
                className="p-2 border-2 border-black bg-white hover:bg-red-600 hover:text-white text-red-600"
                title="Delete"
              >
                <Trash2 size={16} />
              </button>
            )}
          </div>
        </div>

        <h2 className="text-5xl font-black tracking-tighter leading-[0.9] drop-shadow-sm break-words">
          {page.title}
        </h2>

        <div className="space-y-8">
          <div className="flex gap-2 flex-wrap">
            <span className="text-[9px] px-2 py-1 border border-black font-black uppercase bg-slate-50">
              {page.vibe || "Unknown Style"}
            </span>
            {page.ml_script && (
              <span className="text-[9px] px-2 py-1 border border-black font-black uppercase bg-slate-50">
                {page.ml_script}
              </span>
            )}
          </div>

          <div className="space-y-3">
            <h4 className="text-[10px] font-black uppercase text-[#cc543a] flex items-center gap-3">
              <AlignLeft size={14} />
              <span className="tracking-widest">Museum Context & Story</span>
            </h4>
            <p className="text-xl leading-snug font-medium text-slate-900 break-words whitespace-pre-wrap">
              {narrative}
            </p>
          </div>

          <div className="bg-[#f8f5f0] p-8 border-4 border-black border-dashed relative overflow-hidden">
            <div className="absolute -top-3 left-4 bg-black text-white px-2 py-0.5 text-[8px] font-black uppercase tracking-widest">
              Archival Record
            </div>
            <p className="serif text-lg leading-relaxed text-slate-700 italic break-words">
              {page.historicalNote}
            </p>
          </div>
        </div>

        {/* Comments Section */}
        {showComments && (
          <div className="border-4 border-black bg-white space-y-4">
            <button
              onClick={toggleComments}
              className="w-full flex items-center justify-between p-4 border-b-2 border-black/10 hover:bg-slate-50"
            >
              <span className="text-[10px] font-black uppercase tracking-widest">
                Comments
              </span>
              <ChevronUp size={16} />
            </button>

            {/* Add comment form */}
            <form
              onSubmit={handleAddComment}
              className="px-4 flex items-center gap-2"
            >
              <input
                type="text"
                value={newComment}
                onChange={(e) => setNewComment(e.target.value)}
                placeholder={user ? "Add a comment..." : "Sign in to comment"}
                className="flex-1 border-2 border-black p-3 text-sm font-medium outline-none focus:border-[#cc543a]"
                disabled={commentSubmitting || !user}
                maxLength={500}
              />
              <button
                type="submit"
                disabled={commentSubmitting || !newComment.trim() || !user}
                className="p-3 bg-black text-white border-2 border-black hover:bg-[#cc543a] transition-colors disabled:opacity-50"
              >
                {commentSubmitting ? (
                  <Loader2 size={16} className="animate-spin" />
                ) : (
                  <Send size={16} />
                )}
              </button>
            </form>
            {!user && (
              <p className="px-4 text-[10px] font-bold uppercase tracking-widest text-slate-500">
                Sign in from the account page to comment.
              </p>
            )}

            {/* Comments list */}
            <div className="px-4 pb-4 space-y-3 max-h-64 overflow-y-auto">
              {commentsLoading ? (
                <div className="flex justify-center py-4">
                  <Loader2 size={20} className="animate-spin text-[#cc543a]" />
                </div>
              ) : comments.length === 0 ? (
                <p className="text-[10px] font-bold uppercase text-slate-400 text-center py-4">
                  No comments yet. Be the first.
                </p>
              ) : (
                comments.map((comment) => (
                  <div
                    key={comment.id}
                    className="border-l-2 border-black/10 pl-3 space-y-1"
                  >
                    <p className="text-[9px] font-black uppercase tracking-widest text-[#cc543a]">
                      {comment.commenter_name || "Anonymous"}
                    </p>
                    <p className="text-sm font-medium text-slate-900">
                      {comment.content}
                    </p>
                    <p className="text-[9px] font-bold text-slate-400 uppercase">
                      {new Date(comment.created_at).toLocaleDateString()}
                    </p>
                  </div>
                ))
              )}
            </div>
          </div>
        )}

        {!showComments && (
          <button
            onClick={toggleComments}
            className="flex items-center gap-2 text-[10px] font-black uppercase text-slate-400 hover:text-black transition-colors"
          >
            <ChevronDown size={14} />
            Show Comments ({page.comments_count || 0})
          </button>
        )}

        {revisits.length > 0 && (
          <div className="space-y-3">
            <h4 className="text-[10px] font-black uppercase tracking-widest text-[#cc543a]">
              Before / After
            </h4>
            {revisits.map((revisit) => (
              <BeforeAfterSlider key={revisit.id} revisit={revisit} />
            ))}
          </div>
        )}

        {similar.length > 0 && (
          <div className="space-y-3">
            <h4 className="text-[10px] font-black uppercase tracking-widest text-[#cc543a]">
              Similar Lettering
            </h4>
            <div className="flex gap-3 overflow-x-auto pb-2">
              {similar.map((item) => (
                <a
                  key={item.id}
                  href={`#page-${item.id}`}
                  className="min-w-24 border-2 border-black p-1 bg-white hover:-translate-y-0.5 transition-transform"
                >
                  <img
                    src={item.thumbnail || item.image_url}
                    alt={item.detected_text || "Similar lettering"}
                    className="w-24 h-24 object-cover border border-black/20"
                  />
                </a>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default ZinePage;
