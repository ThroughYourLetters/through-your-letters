import React from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useParams } from "react-router-dom";
import { ArrowLeft, Loader2, User } from "lucide-react";
import { api } from "../lib/api";

const ContributorProfile: React.FC<{ onBack?: () => void }> = ({ onBack }) => {
  const navigate = useNavigate();
  const { tag } = useParams<{ tag: string }>();
  const profile = useQuery({
    queryKey: ["contributor", tag],
    enabled: !!tag,
    queryFn: ({ signal }) => api.getContributor(tag!, 50, 0, signal),
  });
  const letterings = profile.data?.letterings ?? [];
  const totalCount = profile.data?.total_count;

  const handleBack = () => {
    if (onBack) {
      onBack();
      return;
    }
    navigate(-1);
  };

  if (!tag) {
    return (
      <div className="text-center py-20 text-stone-500 font-semibold uppercase">
        Contributor not found
      </div>
    );
  }

  return (
    <div className="space-y-12 pb-24">
      <div className="flex items-center gap-4">
        <button
          onClick={handleBack}
          aria-label="Back from contributor profile"
          className="icon-btn"
        >
          <ArrowLeft size={20} />
        </button>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-3">
            <div className="shrink-0 w-12 h-12 bg-ink text-white flex items-center justify-center border border-line">
              <User size={24} />
            </div>
            <div className="min-w-0">
              <h2 className="font-display font-semibold text-3xl tracking-tight break-words">
                {tag}
              </h2>
              {totalCount !== undefined && (
                <p className="text-xs font-semibold uppercase text-muted tracking-widest">
                  {totalCount} contribution{totalCount !== 1 ? "s" : ""}{" "}
                  archived
                </p>
              )}
            </div>
          </div>
        </div>
      </div>

      {profile.isPending ? (
        <div
          role="status"
          aria-label="Loading contributor"
          className="flex justify-center py-20"
        >
          <Loader2 size={32} className="animate-spin text-terracotta" />
        </div>
      ) : profile.error ? (
        <p role="alert">
          Contributions could not be loaded.{" "}
          <button className="underline" onClick={() => void profile.refetch()}>
            Try again
          </button>
        </p>
      ) : letterings.length === 0 ? (
        <p className="text-center text-muted font-semibold uppercase text-sm py-20">
          No approved contributions yet
        </p>
      ) : (
        <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-6">
          {letterings.map((item) => (
            <button
              key={item.id}
              onClick={() => navigate(`/lettering/${item.id}`)}
              className="min-w-0 group bg-surface border border-line p-3 shadow-soft hover:-translate-y-1 transition-all text-left"
            >
              <img
                src={item.thumbnail_urls.small || item.image_url}
                className="aspect-square w-full object-cover border border-line transition-all"
                alt={item.detected_text || "Lettering"}
              />
              <p className="text-xs font-semibold uppercase truncate text-ink mt-3">
                {item.detected_text || "Street Discovery"}
              </p>
              <p className="text-xs font-bold text-muted mt-1">
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
