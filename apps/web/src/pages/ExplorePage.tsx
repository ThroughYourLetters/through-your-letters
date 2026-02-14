import React, { useState, useEffect, useRef, useCallback } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import { ZinePageData, Lettering } from "../types";
import { api } from "../lib/api";
import { useInfiniteGallery } from "../hooks/useLetteringGallery";
import { useWebSocket } from "../hooks/useWebSocket";
import { useCityStore } from "../store/useCityStore";
import { useToastStore } from "../store/useToastStore";
import ZinePage from "../components/ZinePage";
import SearchBar from "../components/SearchBar";
import FilterBar from "../components/FilterBar";
import ImageLightbox from "../components/ImageLightbox";
import { Globe, Loader2 } from "lucide-react";

export const mapLetteringToZinePage = (item: Lettering): ZinePageData => ({
  id: item.id,
  title: item.detected_text || "Street Discovery",
  location: item.pin_code,
  culturalContext:
    item.cultural_context ||
    item.description ||
    "Archived street typography from the city.",
  historicalNote: `Status: ${item.status}. Archived: ${new Date(item.created_at).toLocaleDateString()}`,
  image: item.image_url,
  thumbnail: item.thumbnail_urls.small,
  imageSource: "",
  sourceUrl: "",
  vibe: item.ml_metadata?.style || "Handcrafted",
  readMoreUrl: "",
  isUserContribution: true,
  contributorName: item.contributor_tag,
  description: item.description,
  likes_count: item.likes_count || 0,
  comments_count: item.comments_count || 0,
  ml_script: item.ml_metadata?.script,
  is_owner: item.is_owner,
});

const ExplorePage: React.FC = () => {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const { selectedCityId } = useCityStore();
  const { addToast } = useToastStore();

  const script = searchParams.get("script") || undefined;
  const style = searchParams.get("style") || undefined;
  const sortBy = searchParams.get("sort") || undefined;

  const [searchResults, setSearchResults] = useState<ZinePageData[] | null>(
    null,
  );
  const [lightbox, setLightbox] = useState<{
    imageUrl: string;
    title: string;
    letteringId?: string | number;
  } | null>(null);
  const [liveLetterings, setLiveLetterings] = useState<ZinePageData[]>([]);

  const { data, fetchNextPage, hasNextPage, isFetchingNextPage, isLoading } =
    useInfiniteGallery(selectedCityId, script, style, sortBy);

  const sentinelRef = useRef<HTMLDivElement>(null);

  const handleIntersection = useCallback(
    (entries: IntersectionObserverEntry[]) => {
      if (entries[0].isIntersecting && hasNextPage && !isFetchingNextPage) {
        fetchNextPage();
      }
    },
    [hasNextPage, isFetchingNextPage, fetchNextPage],
  );

  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel) return;
    const observer = new IntersectionObserver(handleIntersection, {
      rootMargin: "400px",
    });
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [handleIntersection]);

  const allLetterings: ZinePageData[] =
    data?.pages.flatMap((page) =>
      page.letterings.map(mapLetteringToZinePage),
    ) ?? [];

  const displayItems = searchResults || allLetterings;

  useWebSocket(
    useCallback((payload: unknown) => {
      const data = payload as { type?: string; id?: string };
      if (data?.type !== "PROCESSED" || !data.id) return;

      api
        .getLettering(data.id)
        .then((item) => {
          const mapped = mapLetteringToZinePage(item);
          setLiveLetterings((prev) => {
            const filtered = prev.filter(
              (p) => String(p.id) !== String(mapped.id),
            );
            return [mapped, ...filtered].slice(0, 20);
          });
        })
        .catch(() => {});
    }, []),
  );

  const handleDelete = async (id: string | number) => {
    if (!window.confirm("Delete this upload permanently?")) return;
    try {
      await api.deleteOwnLettering(id);
      setLiveLetterings((prev) =>
        prev.filter((p) => String(p.id) !== String(id)),
      );
      setSearchResults((prev) =>
        prev ? prev.filter((p) => String(p.id) !== String(id)) : prev,
      );
      addToast("Upload deleted", "success");
    } catch {
      addToast("Delete failed", "error");
    }
  };

  return (
    <>
      <Helmet>
        <title>The Gallery | Through Your Letters</title>
      </Helmet>

      {lightbox && (
        <ImageLightbox
          imageUrl={lightbox.imageUrl}
          title={lightbox.title}
          letteringId={lightbox.letteringId}
          onClose={() => setLightbox(null)}
        />
      )}

      <div className="space-y-40 pb-24">
        <section className="space-y-12">
          <div className="flex justify-between items-end border-b-4 border-black pb-8">
            <h2 className="text-4xl md:text-6xl font-black uppercase tracking-tighter">
              {searchResults ? "Search Results" : "The Gallery"}
            </h2>
            <button
              onClick={() => navigate("/contribute")}
              className="bg-[#cc543a] text-white px-6 py-3 text-[10px] font-black uppercase brutalist-shadow-sm hover:bg-black transition-all"
            >
              Add Discovery
            </button>
          </div>
          <SearchBar
            onResults={(results) =>
              setSearchResults(results.map(mapLetteringToZinePage))
            }
            onClear={() => setSearchResults(null)}
          />
          <FilterBar />
          {isLoading && !searchResults ? (
            <Loader2 className="animate-spin mx-auto text-[#cc543a]" />
          ) : (
            <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-5 gap-6">
              {displayItems.slice(0, 10).map((page, idx) => (
                <div
                  key={page.id}
                  className={`group bg-white border-2 border-black p-3 brutalist-shadow-sm hover:-translate-y-1 transition-all ${idx === 0 ? "md:col-span-2 md:row-span-2" : ""}`}
                >
                  <button
                    onClick={() => navigate(`/lettering/${page.id}`)}
                    className="block w-full text-left space-y-4"
                  >
                    <img
                      src={page.thumbnail || page.image}
                      className="aspect-square w-full object-cover border border-black grayscale group-hover:grayscale-0"
                      alt={page.title}
                    />
                    <p className="text-[11px] font-black uppercase truncate text-black">
                      {page.title}
                    </p>
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>

        <section className="bg-black text-white p-10 brutalist-shadow space-y-8 relative overflow-hidden group">
          <div className="flex items-center gap-3 text-[#d4a017]">
            <Globe size={20} />
            <h4 className="text-[11px] font-black uppercase tracking-widest">
              Museum Access
            </h4>
          </div>
          <p className="text-sm font-bold text-slate-300">
            Browse the complete archive to discover documented typographic
            stories from the city.
          </p>
          <button
            onClick={() =>
              document
                .getElementById("archive-root")
                ?.scrollIntoView({ behavior: "smooth" })
            }
            className="bg-[#cc543a] px-5 py-4 text-[11px] font-black uppercase hover:bg-white hover:text-black transition-all"
          >
            Enter Archive
          </button>
        </section>

        <div id="archive-root" className="space-y-32">
          {[...liveLetterings, ...displayItems]
            .filter(
              (page, index, arr) =>
                index ===
                arr.findIndex((p) => String(p.id) === String(page.id)),
            )
            .map((page) => (
              <ZinePage
                key={page.id}
                page={page}
                onDelete={handleDelete}
                onImageClick={() =>
                  setLightbox({
                    imageUrl: page.image,
                    title: page.title,
                    letteringId: page.id,
                  })
                }
                onContributorClick={
                  page.contributorName
                    ? () => navigate(`/contributor/${page.contributorName}`)
                    : undefined
                }
              />
            ))}

          {!searchResults && (
            <div ref={sentinelRef} className="flex justify-center py-8">
              {isFetchingNextPage && (
                <Loader2 size={24} className="animate-spin text-[#cc543a]" />
              )}
              {!hasNextPage && allLetterings.length > 0 && (
                <p className="text-[10px] font-black uppercase text-slate-300 tracking-widest">
                  End of archive
                </p>
              )}
            </div>
          )}
        </div>
      </div>
    </>
  );
};

export default ExplorePage;
