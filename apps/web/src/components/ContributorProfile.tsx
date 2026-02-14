import React, { useState, useEffect } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { ArrowLeft, Loader2, User } from "lucide-react";
import { api } from "../lib/api";
import { Lettering } from "../types";

const ContributorProfile: React.FC<{ onBack?: () => void }> = ({ onBack }) => {
  const navigate = useNavigate();
  const { tag } = useParams<{ tag: string }>();
  const [letterings, setLetterings] = useState<Lettering[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!tag) {
      setLoading(false);
      return;
    }

    setLoading(true);
    api
      .getContributor(tag)
      .then((data) => {
        setLetterings(data.letterings);
        setTotalCount(data.total_count);
      })
      .catch(() => {
        setLetterings([]);
        setTotalCount(0);
      })
      .finally(() => setLoading(false));
  }, [tag]);

  const handleBack = () => {
    if (onBack) {
      onBack();
      return;
    }
    navigate(-1);
  };

  if (!tag) {
    return (
      <div className="text-center py-20 text-slate-500 font-black uppercase">
        Contributor not found
      </div>
    );
  }

  return (
    <div className="space-y-12 pb-24">
      <div className="flex items-center gap-4">
        <button
          onClick={handleBack}
          className="p-2 border-2 border-black bg-white hover:bg-black hover:text-white transition-colors"
        >
          <ArrowLeft size={20} />
        </button>
        <div className="flex-1">
          <div className="flex items-center gap-3">
            <div className="w-12 h-12 bg-black text-white flex items-center justify-center border-2 border-black">
              <User size={24} />
            </div>
            <div>
              <h2 className="text-3xl font-black uppercase tracking-tighter">
                {tag}
              </h2>
              <p className="text-[10px] font-black uppercase text-slate-400 tracking-widest">
                {totalCount} contribution{totalCount !== 1 ? "s" : ""} archived
              </p>
            </div>
          </div>
        </div>
      </div>

      {loading ? (
        <div className="flex justify-center py-20">
          <Loader2 size={32} className="animate-spin text-[#cc543a]" />
        </div>
      ) : letterings.length === 0 ? (
        <p className="text-center text-slate-400 font-black uppercase text-sm py-20">
          No approved contributions yet
        </p>
      ) : (
        <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-6">
          {letterings.map((item) => (
            <button
              key={item.id}
              onClick={() => navigate(`/lettering/${item.id}`)}
              className="group bg-white border-2 border-black p-3 brutalist-shadow-sm hover:-translate-y-1 transition-all text-left"
            >
              <img
                src={item.thumbnail_urls.small || item.image_url}
                className="aspect-square w-full object-cover border border-black grayscale group-hover:grayscale-0 transition-all"
                alt={item.detected_text || "Lettering"}
              />
              <p className="text-[11px] font-black uppercase truncate text-black mt-3">
                {item.detected_text || "Street Discovery"}
              </p>
              <p className="text-[9px] font-bold text-slate-400 mt-1">
                {item.pin_code}
              </p>
            </button>
          ))}
        </div>
      )}
    </div>
  );
};

export default ContributorProfile;
