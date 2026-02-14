import React, { useState, useEffect } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import { ArrowLeft, FolderOpen, User, Calendar, Loader2 } from "lucide-react";
import { api } from "../lib/api";

const CollectionDetailPage: React.FC = () => {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [collection, setCollection] = useState<any>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!id) return;
    api.getCollection(id)
      .then(setCollection)
      .catch(() => navigate("/community"))
      .finally(() => setLoading(false));
  }, [id, navigate]);

  if (loading) return <div className="flex justify-center py-40"><Loader2 className="animate-spin text-[#cc543a]" size={48} /></div>;

  return (
    <>
      <Helmet><title>{collection.name} | Through Your Letters</title></Helmet>
      <div className="space-y-12 pb-24 animate-in">
        <div className="flex items-center gap-4">
          <button onClick={() => navigate(-1)} className="p-2 border-2 border-black bg-white hover:bg-black hover:text-white transition-colors">
            <ArrowLeft size={20} />
          </button>
          <div className="flex-1">
            <div className="flex items-center gap-3">
              <FolderOpen className="text-[#cc543a]" size={28} />
              <h1 className="text-4xl font-black uppercase tracking-tighter">{collection.name}</h1>
            </div>
            <div className="flex items-center gap-4 mt-2 text-[10px] font-black uppercase text-slate-400 tracking-widest">
              <span className="flex items-center gap-1"><User size={12}/> {collection.creator_tag}</span>
              <span className="flex items-center gap-1"><Calendar size={12}/> {new Date(collection.created_at).toLocaleDateString()}</span>
              <span>{collection.items?.length || 0} Specimens</span>
            </div>
          </div>
        </div>

        <div className="bg-white border-4 border-black p-8 brutalist-shadow-sm">
          <p className="text-xl font-medium leading-relaxed italic text-slate-700">
            {collection.description || "No description provided for this collection."}
          </p>
        </div>

        <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-6">
          {collection.items?.map((item: any) => (
            <Link key={item.id} to={`/lettering/${item.id}`} className="group bg-white border-2 border-black p-3 brutalist-shadow-sm hover:-translate-y-1 transition-all">
              <img src={item.thumbnail || item.image_url} className="aspect-square w-full object-cover border border-black grayscale group-hover:grayscale-0 transition-all" alt="specimen" />
              <p className="text-[11px] font-black uppercase truncate mt-3">{item.detected_text || "Street Discovery"}</p>
              <p className="text-[9px] font-bold text-slate-400 mt-1">@{item.contributor_tag}</p>
            </Link>
          ))}
        </div>
      </div>
    </>
  );
};

export default CollectionDetailPage;