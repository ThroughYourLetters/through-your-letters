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
  ChevronDown,
  ChevronUp,
  FolderPlus,
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
  const [showComments, setShowComments] = useState(false);
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

  const fetchComments = async () => {
    if (!id) return;
    setCommentsLoading(true);
    try {
      const data = await api.getComments(id);
      setComments(data);
    } catch {
      addToast("Failed to load comments", "error");
    } finally {
      setCommentsLoading(false);
    }
  };

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

  if (loading) return <div className="flex justify-center py-40"><Loader2 size={48} className="animate-spin text-terracotta" /></div>;
  if (error || !lettering) return <div className="text-center py-32 space-y-4"><h2 className="font-display font-normal tracking-tight text-2xl">Not Found</h2><Link to="/" className="text-terracotta underline font-semibold">Back to Gallery</Link></div>;

  const title = lettering.detected_text || "Street Discovery";

  return (
    <>
      <Helmet><title>{title} | Through Your Letters</title></Helmet>

      {lightboxOpen && <ImageLightbox imageUrl={lettering.image_url} title={title} letteringId={lettering.id} onClose={() => setLightboxOpen(false)} />}
      {isCurating && <AddToCollectionModal letteringId={lettering.id} onClose={() => { setIsCurating(false); fetchData(lettering.id); }} />}
      {isLinkingRevisit && <LinkRevisitModal originalId={lettering.id} onClose={() => { setIsLinkingRevisit(false); fetchData(lettering.id); }} />}

      <div className="max-w-6xl mx-auto space-y-16 animate-in">
        <button onClick={() => navigate(-1)} className="btn-ghost -ml-3 !px-3">
          <ArrowLeft size={14} /> Back
        </button>

        <div className="grid gap-10 md:grid-cols-12 md:gap-14">
          <div className="md:col-span-7">
            <figure>
              <div className="overflow-hidden rounded-2xl bg-line">
                <img src={lettering.image_url} className="w-full object-cover cursor-zoom-in transition-transform duration-700 hover:scale-[1.02]" alt={title} onClick={() => setLightboxOpen(true)} />
              </div>
              <figcaption className="mt-3 flex justify-between items-center text-sm text-muted">
                <span className="flex items-center gap-1.5">
                  <MapPin size={14} className="text-terracotta" />
                  {lettering.pin_code}
                </span>
                <Link to={`/contributor/${lettering.contributor_tag}`} className="font-medium text-ink hover:text-terracotta transition-colors">By {lettering.contributor_tag}</Link>
              </figcaption>
            </figure>
          </div>

          <div className="md:col-span-5 flex flex-col gap-7 md:pt-2">
            <div className="flex flex-col gap-5">
              <span className="chip self-start border-terracotta/30 bg-terracotta-soft text-terracotta-dark">
                {lettering.ml_metadata?.style || "Handcrafted"}
              </span>
              <div className="flex gap-2 flex-wrap">
                <button onClick={handleLike} className={`icon-btn px-3 ${liked ? "!border-terracotta text-terracotta" : ""}`}>
                  <Heart size={16} fill={liked ? "currentColor" : "none"} />
                  <span className="text-xs font-semibold">{likesCount}</span>
                </button>
                <button onClick={() => setIsCurating(true)} className="icon-btn" title="Curate"><FolderPlus size={16} /></button>
                <button onClick={() => setIsLinkingRevisit(true)} className="icon-btn" title="Link Revisit"><LinkIcon size={16} /></button>
                <button onClick={handleShare} className="icon-btn"><Share2 size={16} /></button>
                <a href={`${API_BASE_URL}/api/v1/letterings/${lettering.id}/download`} target="_blank" rel="noreferrer" className="icon-btn"><Download size={16} /></a>
                <button onClick={handleReport} className="icon-btn text-yellow-700" title="Report"><Flag size={16} /></button>
                {lettering.is_owner && <button onClick={handleDelete} className="icon-btn text-red-600"><Trash2 size={16} /></button>}
              </div>
            </div>

            <h1 className="display text-5xl md:text-6xl leading-[1] break-words">{title}</h1>
            <p className="text-lg leading-relaxed text-ink/90 break-words whitespace-pre-wrap">{lettering.description || lettering.cultural_context || "Archived street typography."}</p>

            <div className="card overflow-hidden">
              <button 
                onClick={() => { 
                  if(!showComments) fetchComments(); 
                  setShowComments(!showComments); 
                }} 
                className="w-full flex items-center justify-between px-5 py-4 hover:bg-paper transition-colors rounded-none"
              >
                <span className="eyebrow">
                  Notes & Comments ({lettering.comments_count || 0})
                </span>
                {showComments ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
              </button>
              
              {showComments && (
                <div className="p-4 border-t border-line space-y-4">
                  {user && (
                    <form onSubmit={handleAddComment} className="flex gap-2">
                      <input type="text" value={newComment} onChange={e => setNewComment(e.target.value)} placeholder="Add a note..." className="field rounded-full" disabled={commentSubmitting} />
                      <button type="submit" disabled={!newComment.trim() || commentSubmitting} className="btn-dark !px-3">
                        {commentSubmitting ? <Loader2 size={16} className="animate-spin" /> : <Send size={16} />}
                      </button>
                    </form>
                  )}
                  
                  <div className="space-y-4 max-h-60 overflow-y-auto">
                    {commentsLoading ? (
                      <div className="flex justify-center py-4"><Loader2 size={20} className="animate-spin text-terracotta" /></div>
                    ) : (
                      comments.map(c => (
                        <div key={c.id} className="border-l-2 border-terracotta/30 pl-3">
                          <p className="text-sm font-semibold">{c.commenter_name}</p>
                          <p className="text-sm text-stone-800">{c.content}</p>
                        </div>
                      ))
                    )}
                  </div>
                </div>
              )}
            </div>

            {revisits.length > 0 && (
              <div className="space-y-3">
                <h4 className="eyebrow text-terracotta">Temporal History</h4>
                {revisits.map((rev) => <BeforeAfterSlider key={rev.id} revisit={rev} />)}
              </div>
            )}
          </div>
        </div>

        {/* Similar Gallery - Restore this section to read the 'similar' variable */}
        {similar.length > 0 && (
          <div className="space-y-6 border-t border-line pt-12">
            <p className="eyebrow -mb-4">From the archive</p>
            <h3 className="display text-3xl md:text-4xl">Related Findings</h3>
            <div className="grid grid-cols-3 md:grid-cols-6 gap-3">
              {similar.map((item) => (
                <Link key={item.id} to={`/lettering/${item.id}`} className="overflow-hidden rounded-xl bg-line group">
                  <img 
                    src={item.thumbnail || item.image_url} 
                    alt="Similar Specimen" 
                    className="w-full aspect-square object-cover transition-transform duration-500 group-hover:scale-105" 
                  />
                </Link>
              ))}
            </div>
          </div>
        )}
      </div>
    </>
  );
};

export default LetteringDetailPage;