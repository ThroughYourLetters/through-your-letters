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
import { useLocaleStore } from "../store/useLocaleStore";
import {
  ArrowDown,
  ArrowRight,
  Globe,
  Loader2,
  Map as MapIcon,
  Plus,
} from "lucide-react";

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
  const heroItems = allLetterings.slice(0, 3);
  const { t } = useLocaleStore();

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

      <div className="space-y-20 md:space-y-28">
        <section className="grid gap-10 md:grid-cols-12 md:items-end pt-2">
          <div className="md:col-span-8 space-y-6">
            <p className="eyebrow flex items-center gap-3">
              <span className="h-px w-8 bg-terracotta" />
              Volume 01 · The Archive
            </p>
            <h1 className="display text-5xl leading-[0.95] sm:text-6xl md:text-7xl lg:text-8xl">
              Every street has
              <br />
              <em className="text-terracotta">a hand</em> that wrote it.
            </h1>
            <p className="max-w-xl text-base md:text-lg leading-relaxed text-muted">
              {t("header_tagline")}
            </p>
            <div className="flex flex-wrap gap-3 pt-2">
              <button onClick={() => navigate("/contribute")} className="btn-primary">
                <Plus size={16} />
                Add Discovery
              </button>
              <button onClick={() => navigate("/map")} className="btn-secondary">
                <MapIcon size={16} />
                Browse the map
              </button>
            </div>
          </div>
          {heroItems.length > 0 && (
            <div className="hidden md:grid md:col-span-4 grid-cols-2 gap-3">
              {heroItems.map((page, idx) => (
                <button
                  key={page.id}
                  onClick={() => navigate(`/lettering/${page.id}`)}
                  className={`group overflow-hidden rounded-2xl bg-line ${idx === 0 ? "col-span-2 aspect-[16/10]" : "aspect-square"}`}
                  aria-label={page.title}
                >
                  <img
                    src={page.thumbnail || page.image}
                    alt={page.title}
                    className="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
                  />
                </button>
              ))}
            </div>
          )}
        </section>

        <section className="space-y-8">
          <div className="sticky top-16 md:top-[6.25rem] z-30 -mx-4 sm:-mx-6 lg:-mx-10 px-4 sm:px-6 lg:px-10 py-3 bg-paper/90 backdrop-blur-md border-b border-line/70 space-y-3">
            <SearchBar
              onResults={(results) =>
                setSearchResults(results.map(mapLetteringToZinePage))
              }
              onClear={() => setSearchResults(null)}
            />
            <FilterBar />
          </div>

          <div className="flex items-end justify-between gap-4">
            <div>
              <p className="eyebrow mb-2">
                {searchResults ? "Search" : "Recently archived"}
              </p>
              <h2 className="display text-4xl md:text-5xl">
                {searchResults ? "Search Results" : "The Gallery"}
              </h2>
            </div>
            <button
              onClick={() => navigate("/contribute")}
              className="btn-secondary hidden sm:inline-flex"
            >
              Add Discovery
              <ArrowRight size={16} />
            </button>
          </div>

          {isLoading && !searchResults ? (
            <div className="flex justify-center py-16">
              <Loader2 className="animate-spin text-terracotta" />
            </div>
          ) : displayItems.length === 0 ? (
            <div className="card py-16 text-center">
              <p className="display text-2xl">Nothing here yet</p>
              <p className="mt-2 text-sm text-muted">
                Try a different filter, or add the first discovery.
              </p>
            </div>
          ) : (
            <div className="masonry">
              {displayItems.slice(0, 12).map((page) => (
                <button
                  key={page.id}
                  onClick={() => navigate(`/lettering/${page.id}`)}
                  className="group block w-full text-left"
                >
                  <div className="overflow-hidden rounded-2xl bg-line">
                    <img
                      src={page.thumbnail || page.image}
                      className="w-full object-cover transition-transform duration-700 group-hover:scale-[1.03]"
                      alt={page.title}
                      loading="lazy"
                    />
                  </div>
                  <div className="mt-3 flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <p className="font-display text-lg leading-tight truncate group-hover:text-terracotta transition-colors">
                        {page.title}
                      </p>
                      <p className="mt-1 text-xs text-muted truncate">
                        {page.location}
                        {page.vibe ? ` · ${page.vibe}` : ""}
                      </p>
                    </div>
                    {page.ml_script && (
                      <span className="chip shrink-0">{page.ml_script}</span>
                    )}
                  </div>
                </button>
              ))}
            </div>
          )}
        </section>

        <section className="relative overflow-hidden rounded-3xl bg-ink px-6 py-12 md:px-14 md:py-16 text-paper">
          <div className="absolute -right-10 -top-16 font-display text-[16rem] leading-none text-white/5 select-none pointer-events-none">
            ಅ
          </div>
          <div className="relative grid gap-8 md:grid-cols-2 md:items-end">
            <div className="space-y-4">
              <p className="eyebrow flex items-center gap-2 text-ochre">
                <Globe size={14} />
                Museum Access
              </p>
              <p className="display text-3xl md:text-4xl text-paper">
                Browse the complete archive to discover documented typographic
                stories from the city.
              </p>
            </div>
            <div className="md:text-right">
              <button
                onClick={() =>
                  document
                    .getElementById("archive-root")
                    ?.scrollIntoView({ behavior: "smooth" })
                }
                className="btn-primary"
              >
                Enter Archive
                <ArrowDown size={16} />
              </button>
            </div>
          </div>
        </section>

        <div id="archive-root" className="scroll-mt-40 space-y-6">
          <div className="border-b border-line pb-6">
            <p className="eyebrow mb-2">Field notes</p>
            <h2 className="display text-4xl md:text-5xl">The Archive</h2>
          </div>
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
                <Loader2 size={24} className="animate-spin text-terracotta" />
              )}
              {!hasNextPage && allLetterings.length > 0 && (
                <p className="eyebrow">End of archive</p>
              )}
            </div>
          )}
        </div>
      </div>
    </>
  );
};

export default ExplorePage;
