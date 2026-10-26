import React, { useState, useEffect, useCallback } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import {
  ArrowLeft,
  MapPin,
  Heart,
  Share2,
  Download,
  Trash2,
  Send,
  Loader2,
  FolderPlus,
  Maximize2,
  Link as LinkIcon,
  Flag,
} from "lucide-react";
import { api } from "../lib/api";
import { Lettering, Comment, RevisitLink } from "../types";
import { API_BASE_URL } from "../constants";
import { useToastStore } from "../store/useToastStore";
import { useAuthStore } from "../store/useAuthStore";
import BeforeAfterSlider from "../components/BeforeAfterSlider";
import ImageLightbox from "../components/ImageLightbox";
import AddToCollectionModal from "../components/AddToCollectionModal";
import LinkRevisitModal from "../components/LinkRevisitModal";

const LetteringDetailPage: React.FC = () => {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { addToast } = useToastStore();
  const { user, hydrated, hydrate } = useAuthStore();

  const [lettering, setLettering] = useState<Lettering | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [liked, setLiked] = useState(false);
  const [likesCount, setLikesCount] = useState(0);
  const [likeLoading, setLikeLoading] = useState(false);
  const [isCurating, setIsCurating] = useState(false);
  const [isLinkingRevisit, setIsLinkingRevisit] = useState(false);

  const [comments, setComments] = useState<Comment[]>([]);
  const [newComment, setNewComment] = useState("");
  const [commentSubmitting, setCommentSubmitting] = useState(false);
  const [commentsLoading, setCommentsLoading] = useState(false);

  const [revisits, setRevisits] = useState<RevisitLink[]>([]);
  const [similar, setSimilar] = useState<any[]>([]); // This variable is now read in the JSX below
  const [lightboxOpen, setLightboxOpen] = useState(false);

  const fetchData = useCallback(async (targetId: string) => {
    setLoading(true);
    try {
      const data = await api.getLettering(targetId);
      setLettering(data);
      setLikesCount(data.likes_count || 0);

      const [revisitData, similarData] = await Promise.all([
        api.getRevisits(targetId),
        api.getSimilar(targetId),
      ]);
      setRevisits(revisitData.revisits || []);
      setSimilar(similarData.similar || []);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Archive link broken");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { if (id) fetchData(id); }, [id, fetchData]);
  useEffect(() => { if (!hydrated) hydrate(); }, [hydrated, hydrate]);

  const handleLike = async () => {
    if (!id || likeLoading) return;
    setLikeLoading(true);
    try {
      const data = await api.toggleLike(id);
      setLiked(data.liked);
      setLikesCount(data.likes_count);
    } catch {
      addToast("Failed to toggle like", "error");
    } finally {
      setLikeLoading(false);
    }
  };

  const handleShare = async () => {
    const url = window.location.href;
    try {
      await navigator.share({
        title: `Through Your Letters: ${lettering?.detected_text || "Street Discovery"}`,
        url,
      });
    } catch {
      await navigator.clipboard.writeText(url);
      addToast("Link copied", "success");
    }
  };

  const handleReport = async () => {
    if (!id) return;
    const reason = window.prompt("Reason for report:");
    if (!reason) return;
    try {
      await api.reportLettering(id, reason);
      addToast("Reported for review", "success");
    } catch {
      addToast("Report failed", "error");
    }
  };

  const handleDelete = async () => {
    if (!id || !window.confirm("Delete permanently?")) return;
    try {
      await api.deleteOwnLettering(id);
      addToast("Deleted", "success");
      navigate("/");
    } catch (err) {
      addToast(err instanceof Error ? err.message : "Delete failed", "error");
    }
  };

  useEffect(() => {
    if (!id) return;
    let active = true;
    setCommentsLoading(true);
    api
      .getComments(id)
      .then((data) => {
        if (active) setComments(data);
      })
      .catch(() => {})
      .finally(() => {
        if (active) setCommentsLoading(false);
      });
    return () => {
      active = false;
    };
  }, [id]);

  const handleAddComment = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!user || !id || !newComment.trim()) return;
    setCommentSubmitting(true);
    try {
      const comment = await api.addComment(id, newComment.trim());
      setComments((prev) => [comment, ...prev]);
      setNewComment("");
    } catch {
      addToast("Failed to add comment", "error");
    } finally {
      setCommentSubmitting(false);
    }
  };

  if (loading)
    return (
      <div className="grid gap-10 md:grid-cols-12 md:gap-14" aria-busy="true">
        <div className="skeleton md:col-span-7 aspect-[4/3] rounded-3xl" />
        <div className="md:col-span-5 space-y-4">
          <div className="skeleton h-6 w-24 rounded-full" />
          <div className="skeleton h-14 w-3/4" />
          <div className="skeleton h-24" />
        </div>
      </div>
    );
  if (error || !lettering)
    return (
      <div className="text-center py-32 space-y-4">
        <h2 className="display text-3xl">Not Found</h2>
        <Link to="/" className="btn-secondary">
          Back to Gallery
        </Link>
      </div>
    );

  const title = lettering.detected_text || "Street Discovery";
  const meta: Array<{ label: string; value: React.ReactNode }> = [
    {
      label: "Location",
      value: (
        <span className="inline-flex items-center gap-1.5">
          <MapPin size={14} className="text-terracotta" />
          {lettering.pin_code}
        </span>
      ),
    },
    {
      label: "Contributor",
      value: (
        <Link
          to={`/contributor/${lettering.contributor_tag}`}
          className="font-medium hover:text-terracotta transition-colors"
        >
          @{lettering.contributor_tag}
        </Link>
      ),
    },
    { label: "Style", value: lettering.ml_metadata?.style || "Handcrafted" },
    ...(lettering.ml_metadata?.script
      ? [{ label: "Script", value: lettering.ml_metadata.script }]
      : []),
    {
      label: "Archived",
      value: new Date(lettering.created_at).toLocaleDateString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
      }),
    },
  ];

  return (
    <>
      <Helmet><title>{title} | Through Your Letters</title></Helmet>

      {lightboxOpen && <ImageLightbox imageUrl={lettering.image_url} title={title} letteringId={lettering.id} onClose={() => setLightboxOpen(false)} />}
      {isCurating && <AddToCollectionModal letteringId={lettering.id} onClose={() => { setIsCurating(false); fetchData(lettering.id); }} />}
      {isLinkingRevisit && <LinkRevisitModal originalId={lettering.id} onClose={() => { setIsLinkingRevisit(false); fetchData(lettering.id); }} />}

      <div className="space-y-16 animate-in">
        <button onClick={() => navigate(-1)} className="btn-ghost -ml-3 !px-3 -mb-10">
          <ArrowLeft size={16} /> Back
        </button>

        <div className="grid gap-10 md:grid-cols-12 md:gap-14">
          <div className="md:col-span-7">
            <button
              onClick={() => setLightboxOpen(true)}
              className="group relative block w-full overflow-hidden rounded-3xl bg-line cursor-zoom-in"
              aria-label="View full size"
            >
              <img src={lettering.image_url} className="w-full object-cover transition-transform duration-700 group-hover:scale-[1.02]" alt={title} />
              <span className="absolute bottom-4 right-4 inline-flex items-center gap-1.5 rounded-full bg-white/90 px-3 py-1.5 text-xs font-semibold text-ink shadow-soft backdrop-blur opacity-0 transition-opacity group-hover:opacity-100">
                <Maximize2 size={13} /> Full size
              </span>
            </button>
          </div>

          <div className="md:col-span-5 md:sticky md:top-24 md:self-start flex flex-col gap-6">
            <div className="space-y-3">
              <span className="chip border-terracotta/30 bg-terracotta-soft text-terracotta-dark">
                {lettering.ml_metadata?.style || "Handcrafted"}
              </span>
              <h1 className="display text-4xl md:text-5xl leading-[1.05] break-words">{title}</h1>
              <p className="text-base md:text-lg leading-relaxed text-ink/80 break-words whitespace-pre-wrap">
                {lettering.description || lettering.cultural_context || "Archived street typography."}
              </p>
            </div>

            <div className="flex flex-wrap items-center gap-2">
              <button
                onClick={handleLike}
                disabled={likeLoading}
                aria-pressed={liked}
                className={liked ? "btn-primary !py-2" : "btn-secondary !py-2"}
              >
                <Heart size={16} fill={liked ? "currentColor" : "none"} />
                {likesCount} {likesCount === 1 ? "like" : "likes"}
              </button>
              <button onClick={() => setIsCurating(true)} className="btn-secondary !py-2">
                <FolderPlus size={16} /> Save
              </button>
              <button onClick={handleShare} className="btn-secondary !py-2">
                <Share2 size={16} /> Share
              </button>
            </div>

            <dl className="card divide-y divide-line text-sm">
              {meta.map((m) => (
                <div key={m.label} className="flex items-center justify-between gap-4 px-5 py-3">
                  <dt className="text-muted">{m.label}</dt>
                  <dd className="text-right text-ink">{m.value}</dd>
                </div>
              ))}
            </dl>

            <div className="flex flex-wrap items-center gap-1 text-sm">
              <button onClick={() => setIsLinkingRevisit(true)} className="btn-ghost !px-3 !py-1.5 !font-medium">
                <LinkIcon size={14} /> Link a revisit
              </button>
              <a
                href={`${API_BASE_URL}/api/v1/letterings/${lettering.id}/download`}
                target="_blank"
                rel="noreferrer"
                className="btn-ghost !px-3 !py-1.5 !font-medium"
              >
                <Download size={14} /> Download
              </a>
              <button onClick={handleReport} className="btn-ghost !px-3 !py-1.5 !font-medium">
                <Flag size={14} /> Report
              </button>
              {lettering.is_owner && (
                <button onClick={handleDelete} className="btn-ghost !px-3 !py-1.5 !font-medium !text-red-600 hover:!bg-red-50">
                  <Trash2 size={14} /> Delete
                </button>
              )}
            </div>
          </div>
        </div>

        <section className="grid gap-10 md:grid-cols-12 md:gap-14 border-t border-line pt-12">
          <div className="md:col-span-7 space-y-5">
            <h2 className="display text-2xl md:text-3xl">
              Notes & Comments{" "}
              <span className="text-muted font-normal">
                ({comments.length || lettering.comments_count || 0})
              </span>
            </h2>
            {user ? (
              <form onSubmit={handleAddComment} className="flex gap-2">
                <input
                  type="text"
                  value={newComment}
                  onChange={(e) => setNewComment(e.target.value)}
                  placeholder="Add a note or a memory about this sign…"
                  aria-label="Add a note"
                  className="field rounded-full"
                  disabled={commentSubmitting}
                  maxLength={500}
                />
                <button type="submit" disabled={!newComment.trim() || commentSubmitting} className="btn-dark">
                  {commentSubmitting ? <Loader2 size={16} className="animate-spin" /> : <Send size={16} />}
                  <span className="hidden sm:inline">Post</span>
                </button>
              </form>
            ) : (
              <div className="card flex flex-wrap items-center justify-between gap-3 px-5 py-4">
                <p className="text-sm text-muted">Sign in to add a note or a memory.</p>
                <Link to="/auth" className="btn-secondary !py-2">Sign In</Link>
              </div>
            )}

            {commentsLoading ? (
              <div className="space-y-3">
                <div className="skeleton h-14" />
                <div className="skeleton h-14" />
              </div>
            ) : comments.length === 0 ? (
              <p className="text-sm text-muted">No notes yet. Be the first to share a story.</p>
            ) : (
              <ul className="space-y-4">
                {comments.map((c) => (
                  <li key={c.id} className="flex gap-3">
                    <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-terracotta-soft text-sm font-semibold uppercase text-terracotta-dark">
                      {(c.commenter_name || "A").charAt(0)}
                    </span>
                    <div className="min-w-0">
                      <p className="text-sm">
                        <span className="font-semibold">{c.commenter_name || "Anonymous"}</span>{" "}
                        <span className="text-xs text-muted">· {new Date(c.created_at).toLocaleDateString()}</span>
                      </p>
                      <p className="text-sm text-ink/85 break-words">{c.content}</p>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </div>

          {revisits.length > 0 && (
            <div className="md:col-span-5 space-y-4">
              <h2 className="display text-2xl md:text-3xl">Temporal History</h2>
              {revisits.map((rev) => <BeforeAfterSlider key={rev.id} revisit={rev} />)}
            </div>
          )}
        </section>

        {similar.length > 0 && (
          <section className="space-y-6 border-t border-line pt-12">
            <div className="flex items-end justify-between">
              <h2 className="display text-2xl md:text-3xl">Related Findings</h2>
              <Link to="/" className="text-sm font-medium text-muted hover:text-ink">View gallery →</Link>
            </div>
            <div className="grid grid-cols-3 md:grid-cols-6 gap-3">
              {similar.map((item) => (
                <Link key={item.id} to={`/lettering/${item.id}`} className="group overflow-hidden rounded-2xl bg-line">
                  <img
                    src={item.thumbnail || item.image_url}
                    alt={item.detected_text || "Similar Specimen"}
                    className="w-full aspect-square object-cover transition-transform duration-500 group-hover:scale-105"
                  />
                </Link>
              ))}
            </div>
          </section>
        )}
      </div>
    </>
  );
};

export default LetteringDetailPage;