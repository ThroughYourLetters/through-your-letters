import React, { useState, useEffect } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import {
  ArrowLeft,
  MapPin,
  Heart,
  MessageCircle,
  Share2,
  Download,
  AlertTriangle,
  Trash2,
  Send,
  Loader2,
  ChevronDown,
  ChevronUp,
  FolderPlus, // Added missing import
  Link as LinkIcon, // Added for revisit linking
} from "lucide-react";
import { api } from "../lib/api";
import { Lettering, Comment, RevisitLink } from "../types";
import { API_BASE_URL } from "../constants";
import { useToastStore } from "../store/useToastStore";
import { useAuthStore } from "../store/useAuthStore";
import BeforeAfterSlider from "../components/BeforeAfterSlider";
import ImageLightbox from "../components/ImageLightbox";
import AddToCollectionModal from "../components/AddToCollectionModal";
import LinkRevisitModal from "../components/LinkRevisitModal"; // Integrated modal

const LetteringDetailPage: React.FC = () => {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { addToast } = useToastStore();
  const { user, hydrated, hydrate } = useAuthStore();

  const [lettering, setLettering] = useState<Lettering | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Interaction State
  const [liked, setLiked] = useState(false);
  const [likesCount, setLikesCount] = useState(0);
  const [likeLoading, setLikeLoading] = useState(false);
  const [isCurating, setIsCurating] = useState(false);
  const [isLinkingRevisit, setIsLinkingRevisit] = useState(false);

  // Comments state
  const [comments, setComments] = useState<Comment[]>([]);
  const [showComments, setShowComments] = useState(false);
  const [newComment, setNewComment] = useState("");
  const [commentsLoading, setCommentsLoading] = useState(false);
  const [commentSubmitting, setCommentSubmitting] = useState(false);
  const [commentsLoaded, setCommentsLoaded] = useState(false);

  // Related Data
  const [revisits, setRevisits] = useState<RevisitLink[]>([]);
  const [similar, setSimilar] = useState<
    Array<{
      id: string;
      thumbnail?: string;
      image_url: string;
      detected_text?: string;
    }>
  >([]);

  // Lightbox
  const [lightboxOpen, setLightboxOpen] = useState(false);

  const fetchData = async (targetId: string) => {
    setLoading(true);
    try {
      const data = await api.getLettering(targetId);
      setLettering(data);
      setLikesCount(data.likes_count || 0);

      // Parallel fetch for secondary data
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
  };

  useEffect(() => {
    if (id) fetchData(id);
  }, [id]);

  useEffect(() => {
    if (!hydrated) hydrate();
  }, [hydrated, hydrate]);

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
    const shareData = {
      title: `Through Your Letters: ${lettering?.detected_text || "Street Discovery"}`,
      text: `Check out this typography artifact from ${lettering?.pin_code}`,
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
      if ((err as Error).name !== "AbortError") addToast("Share failed", "error");
    }
  };

  const handleReport = async () => {
    if (!id) return;
    const reason = window.prompt("Why are you reporting this image?");
    if (!reason) return;
    try {
      await api.reportLettering(id, reason);
      addToast("Report submitted for review", "success");
    } catch {
      addToast("Failed to submit report", "error");
    }
  };

  const handleDelete = async () => {
    if (!id) return;
    if (!window.confirm("Delete this upload permanently?")) return;
    try {
      await api.deleteOwnLettering(id);
      addToast("Upload deleted", "success");
      navigate("/");
    } catch (err) {
      addToast(err instanceof Error ? err.message : "Delete failed", "error");
    }
  };

  const handleAddComment = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!user || !id || !newComment.trim()) return;
    setCommentSubmitting(true);
    try {
      const comment = await api.addComment(id, newComment.trim());
      if (comment.status === "VISIBLE" || !comment.status) {
        setComments((prev) => [comment, ...prev]);
      } else {
        addToast("Comment held for moderator review.", "info");
      }
      setNewComment("");
    } catch {
      addToast("Failed to add comment", "error");
    } finally {
      setCommentSubmitting(false);
    }
  };

  if (loading) return <div className="flex justify-center py-40"><Loader2 size={48} className="animate-spin text-[#cc543a]" /></div>;
  if (error || !lettering) return <div className="text-center py-32 space-y-4"><h2 className="text-2xl font-black uppercase">Not Found</h2><Link to="/" className="text-[#cc543a] font-black uppercase underline">Return to Gallery</Link></div>;

  const title = lettering.detected_text || "Street Discovery";
  const narrative = lettering.description || lettering.cultural_context || "Archived street typography.";

  return (
    <>
      <Helmet><title>{title} | Through Your Letters</title></Helmet>

      {lightboxOpen && (
        <ImageLightbox imageUrl={lettering.image_url} title={title} letteringId={lettering.id} onClose={() => setLightboxOpen(false)} />
      )}

      {isCurating && (
        <AddToCollectionModal letteringId={lettering.id} onClose={() => { setIsCurating(false); addToast("Specimen added to collection", "success"); }} />
      )}

      {isLinkingRevisit && (
        <LinkRevisitModal originalId={lettering.id} onClose={() => { setIsLinkingRevisit(false); fetchData(lettering.id); }} />
      )}

      <div className="max-w-5xl mx-auto space-y-12 pb-24 animate-in">
        <button onClick={() => navigate(-1)} className="flex items-center gap-2 text-[10px] font-black uppercase tracking-widest text-slate-400 hover:text-black transition-colors">
          <ArrowLeft size={14} /> Back
        </button>

        <div className="flex flex-col md:flex-row gap-12">
          <div className="w-full md:w-3/5">
            <div className="p-3 bg-white border-2 border-black brutalist-shadow transition-all hover:rotate-1">
              <img src={lettering.image_url} className="w-full aspect-square object-cover contrast-125 grayscale hover:grayscale-0 transition-all cursor-zoom-in" alt={title} onClick={() => setLightboxOpen(true)} />
              <div className="p-4 flex justify-between items-center border-t border-black/5 mt-2 bg-slate-50/50">
                <div className="flex items-center gap-2">
                  <MapPin size={14} className="text-[#cc543a]" />
                  <span className="text-[10px] font-black uppercase tracking-widest">{lettering.pin_code}</span>
                </div>
                <Link to={`/contributor/${lettering.contributor_tag}`} className="text-[9px] font-black uppercase text-[#cc543a] hover:underline">By {lettering.contributor_tag}</Link>
              </div>
            </div>
          </div>

          <div className="w-full md:w-2/5 flex flex-col space-y-8">
            <div className="flex justify-between items-start gap-4">
              <div className="flex gap-2 flex-wrap">
                <span className="bg-black text-white px-4 py-1.5 text-xs font-black uppercase rotate-1 shadow-[4px_4px_0_0_#cc543a]">
                  {lettering.ml_metadata?.style || "Handcrafted"}
                </span>
              </div>
              <div className="flex gap-2 flex-wrap justify-end">
                <button onClick={handleLike} className={`p-2 border-2 border-black bg-white hover:bg-red-50 transition-colors flex items-center gap-1 ${liked ? "text-[#cc543a]" : "text-slate-400"}`}>
                  <Heart size={16} fill={liked ? "currentColor" : "none"} />
                  <span className="text-[10px] font-black">{likesCount}</span>
                </button>
                <button onClick={() => setIsCurating(true)} className="p-2 border-2 border-black bg-white hover:bg-black hover:text-white transition-colors" title="Curate">
                  <FolderPlus size={16} />
                </button>
                <button onClick={() => setIsLinkingRevisit(true)} className="p-2 border-2 border-black bg-white hover:bg-black hover:text-white transition-colors" title="Link Revisit">
                  <LinkIcon size={16} />
                </button>
                <button onClick={handleShare} className="p-2 border-2 border-black bg-white hover:bg-slate-100"><Share2 size={16} /></button>
                <a href={`${API_BASE_URL}/api/v1/letterings/${lettering.id}/download`} target="_blank" rel="noreferrer" className="p-2 border-2 border-black bg-white hover:bg-slate-100"><Download size={16} /></a>
                {lettering.is_owner && (
                  <button onClick={handleDelete} className="p-2 border-2 border-black bg-white hover:bg-red-600 hover:text-white text-red-600"><Trash2 size={16} /></button>
                )}
              </div>
            </div>

            <h1 className="text-5xl font-black tracking-tighter leading-[0.9] break-words">{title}</h1>

            <div className="space-y-6">
              <p className="text-xl leading-snug font-medium text-slate-900 break-words whitespace-pre-wrap">{narrative}</p>
              <div className="bg-[#f8f5f0] p-6 border-4 border-black border-dashed relative">
                <div className="absolute -top-3 left-4 bg-black text-white px-2 py-0.5 text-[8px] font-black uppercase tracking-widest">Archival Record</div>
                <p className="serif text-sm italic text-slate-700">Status: {lettering.status} // Archived {new Date(lettering.created_at).toLocaleDateString()}</p>
              </div>
            </div>

            {/* Comments Section */}
            <div className="border-4 border-black bg-white">
              <button onClick={() => { if(!showComments && !commentsLoaded) api.getComments(lettering.id).then(setComments); setShowComments(!showComments); }} className="w-full flex items-center justify-between p-4 hover:bg-slate-50 transition-colors">
                <span className="text-[10px] font-black uppercase tracking-widest">Notes & Comments ({lettering.comments_count || 0})</span>
                {showComments ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
              </button>
              
              {showComments && (
                <div className="p-4 border-t-2 border-black space-y-4">
                  <form onSubmit={handleAddComment} className="flex gap-2">
                    <input type="text" value={newComment} onChange={e => setNewComment(e.target.value)} placeholder={user ? "Add a note..." : "Sign in to comment"} className="flex-1 border-2 border-black p-2 text-sm outline-none" disabled={!user || commentSubmitting} />
                    <button type="submit" disabled={!user || !newComment.trim() || commentSubmitting} className="p-2 bg-black text-white border-2 border-black hover:bg-[#cc543a] transition-all">
                      {commentSubmitting ? <Loader2 size={16} className="animate-spin" /> : <Send size={16} />}
                    </button>
                  </form>
                  <div className="space-y-4 max-h-60 overflow-y-auto">
                    {comments.map(c => (
                      <div key={c.id} className="border-l-2 border-black/10 pl-3">
                        <p className="text-[9px] font-black uppercase text-[#cc543a]">{c.commenter_name}</p>
                        <p className="text-sm text-slate-800">{c.content}</p>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>

            {/* Revisits */}
            {revisits.length > 0 && (
              <div className="space-y-3">
                <h4 className="text-[10px] font-black uppercase tracking-widest text-[#cc543a]">Temporal History (Before/After)</h4>
                {revisits.map((revisit) => <BeforeAfterSlider key={revisit.id} revisit={revisit} />)}
              </div>
            )}
          </div>
        </div>

        {/* Similar Gallery */}
        {similar.length > 0 && (
          <div className="space-y-6 border-t-4 border-black pt-12">
            <h3 className="text-2xl font-black uppercase tracking-tighter">Related Findings</h3>
            <div className="grid grid-cols-3 md:grid-cols-6 gap-4">
              {similar.map((item) => (
                <Link key={item.id} to={`/lettering/${item.id}`} className="border-2 border-black p-2 bg-white hover:-translate-y-1 transition-all group">
                  <img src={item.thumbnail || item.image_url} alt="Similar" className="w-full aspect-square object-cover border border-black/20 grayscale group-hover:grayscale-0 transition-all" />
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