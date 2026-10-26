import React, { useState, useEffect, useRef, useCallback } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import { ZinePageData, Lettering } from "../types";
import { api } from "../lib/api";
import { useInfiniteGallery } from "../hooks/useLetteringGallery";
import { useWebSocket } from "../hooks/useWebSocket";
import { useCityStore } from "../store/useCityStore";
import { useToastStore } from "../store/useToastStore";
import SearchBar from "../components/SearchBar";
import FilterBar from "../components/FilterBar";
import ImageLightbox from "../components/ImageLightbox";
import { useLocaleStore } from "../store/useLocaleStore";
import WordCycler from "../components/playful/WordCycler";
import Tilt from "../components/playful/Tilt";
import {
  ArrowRight,
  ArrowUp,
  Dices,
  Globe,
  Heart,
  Loader2,
  Map as MapIcon,
  Maximize2,
  MessageCircle,
  Plus,
  SearchX,
  Trash2,
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
  const { t } = useLocaleStore();
  const [showTop, setShowTop] = useState(false);

  useEffect(() => {
    const onScroll = () => setShowTop(window.scrollY > 1200);
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  const surpriseMe = () => {
    if (allLetterings.length === 0) return;
    const pick = allLetterings[Math.floor(Math.random() * allLetterings.length)];
    navigate(`/lettering/${pick.id}`);
  };

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

  const total = data?.pages[0]?.total ?? 0;
  const liveIds = new Set(liveLetterings.map((p) => String(p.id)));
  const gridItems = [...(searchResults ? [] : liveLetterings), ...displayItems].filter(
    (page, index, arr) =>
      index === arr.findIndex((p) => String(p.id) === String(page.id)),
  );
  const heroItems = allLetterings.slice(0, 3);
  const latest = allLetterings[0];

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

      {showTop && (
        <button
          onClick={() => window.scrollTo({ top: 0, behavior: "smooth" })}
          className="fixed bottom-24 right-4 z-40 flex h-11 w-11 items-center justify-center rounded-full border border-line bg-surface/95 text-ink shadow-lift backdrop-blur transition-transform hover:-translate-y-0.5 md:bottom-8 md:right-8 animate-in"
          aria-label="Back to top"
          title="Back to top"
        >
          <ArrowUp size={18} />
        </button>
      )}

      <div className="space-y-14 md:space-y-20">
        <section className="grid gap-10 lg:grid-cols-12 lg:items-center animate-in">
          <div className="lg:col-span-7 min-w-0 space-y-6">
            <h1 className="display text-[2.75rem] leading-[1.02] sm:text-6xl lg:text-7xl">
              Every street sign holds{" "}
              <em className="text-terracotta">
                <WordCycler />
              </em>
              .
            </h1>
            <p className="max-w-xl text-base md:text-lg leading-relaxed text-muted">
              {t("header_tagline")}
            </p>
            <div className="flex flex-wrap gap-3">
              <button onClick={() => navigate("/contribute")} className="btn-primary">
                <Plus size={16} />
                Add Discovery
              </button>
              <button onClick={() => navigate("/map")} className="btn-secondary">
                <MapIcon size={16} />
                Browse the map
              </button>
              {allLetterings.length > 1 && (
                <button
                  onClick={surpriseMe}
                  className="btn-ghost group"
                  title="Open a random lettering"
                >
                  <Dices size={16} className="transition-transform duration-500 group-hover:rotate-[360deg]" />
                  Surprise me
                </button>
              )}
            </div>

            {heroItems.length > 0 && (
              <div className="no-scrollbar -mx-4 flex gap-3 overflow-x-auto px-4 pt-2 md:hidden">
                {heroItems.map((page) => (
                  <button
                    key={page.id}
                    onClick={() => navigate(`/lettering/${page.id}`)}
                    className="h-28 w-36 shrink-0 overflow-hidden rounded-2xl bg-line"
                    aria-label={page.title}
                  >
                    <img
                      src={page.thumbnail || page.image}
                      alt={page.title}
                      className="h-full w-full object-cover"
                    />
                  </button>
                ))}
              </div>
            )}
          </div>

          {heroItems.length > 0 && (
            <Tilt className="relative hidden md:grid lg:col-span-5 grid-cols-5 grid-rows-2 gap-3 h-[26rem]">
              {heroItems.map((page, idx) => (
                <button
                  key={page.id}
                  onClick={() => navigate(`/lettering/${page.id}`)}
                  className={`group overflow-hidden rounded-3xl bg-line ${idx === 0 ? "col-span-3 row-span-2" : "col-span-2"}`}
                  aria-label={page.title}
                >
                  <img
                    src={page.thumbnail || page.image}
                    alt={page.title}
                    className="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
                  />
                </button>
              ))}
              {latest && (
                <button
                  onClick={() => navigate(`/lettering/${latest.id}`)}
                  className="absolute -bottom-5 left-6 flex max-w-[16rem] animate-float items-center gap-3 rounded-2xl border border-line bg-surface/90 p-2.5 pr-4 text-left shadow-lift backdrop-blur"
                >
                  <img
                    src={latest.thumbnail || latest.image}
                    alt=""
                    className="h-10 w-10 rounded-xl object-cover"
                  />
                  <span className="min-w-0">
                    <span className="block text-[11px] font-medium uppercase tracking-wider text-muted">
                      Latest find
                    </span>
                    <span className="block truncate text-sm font-semibold">
                      {latest.title}
                    </span>
                  </span>
                </button>
              )}
            </Tilt>
          )}
        </section>

        <section className="space-y-6">
          <div className="sticky top-16 z-30 -mx-4 sm:-mx-6 lg:-mx-10 px-4 sm:px-6 lg:px-10 py-3 bg-paper/80 backdrop-blur-xl backdrop-saturate-150 border-b border-line/70 space-y-3">
            <SearchBar
              onResults={(results) =>
                setSearchResults(results.map(mapLetteringToZinePage))
              }
              onClear={() => setSearchResults(null)}
            />
            {!searchResults && (
              <div className="hidden sm:block">
                <FilterBar />
              </div>
            )}
          </div>
          {!searchResults && (
            <div className="sm:hidden">
              <FilterBar />
            </div>
          )}

          <div className="flex items-end justify-between gap-4">
            <div>
              <h2 className="display text-3xl md:text-4xl">
                {searchResults ? "Search results" : "The Gallery"}
              </h2>
              <p className="mt-1 text-sm text-muted">
                {searchResults
                  ? `${searchResults.length} matching ${searchResults.length === 1 ? "entry" : "entries"}`
                  : total > 0
                    ? `Showing ${allLetterings.length} of ${total.toLocaleString()}`
                    : "Recently archived street lettering"}
              </p>
            </div>
          </div>

          {isLoading && !searchResults ? (
            <div className="masonry" aria-busy="true" aria-label="Loading gallery">
              {Array.from({ length: 8 }, (_, i) => (
                <div key={i}>
                  <div className={`skeleton ${["h-64", "h-80", "h-56", "h-72"][i % 4]}`} />
                  <div className="skeleton mt-3 h-4 w-2/3 rounded-full" />
                </div>
              ))}
            </div>
          ) : gridItems.length === 0 ? (
            <div className="card flex flex-col items-center gap-4 px-6 py-16 text-center">
              <span className="flex h-12 w-12 items-center justify-center rounded-2xl bg-terracotta-soft text-terracotta">
                <SearchX size={22} />
              </span>
              <div>
                <p className="display text-2xl">Nothing here yet</p>
                <p className="mt-1 text-sm text-muted">
                  Try a different filter, or add the first discovery.
                </p>
              </div>
              <div className="flex flex-wrap justify-center gap-2">
                <button onClick={() => navigate("/contribute")} className="btn-primary">
                  Add Discovery
                </button>
                {(script || style || sortBy) && (
                  <button onClick={() => navigate("/")} className="btn-secondary">
                    Clear filters
                  </button>
                )}
              </div>
            </div>
          ) : (
            <div className="masonry">
              {gridItems.map((page) => (
                <GalleryCard
                  key={page.id}
                  page={page}
                  isNew={liveIds.has(String(page.id))}
                  onOpen={() => navigate(`/lettering/${page.id}`)}
                  onExpand={() =>
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
                  onDelete={page.is_owner ? () => handleDelete(page.id) : undefined}
                />
              ))}
            </div>
          )}

          {!searchResults && (
            <div ref={sentinelRef} className="flex justify-center py-6">
              {isFetchingNextPage && (
                <Loader2 size={22} className="animate-spin text-terracotta" />
              )}
              {!hasNextPage && allLetterings.length > 0 && (
                <p className="text-sm text-muted">You've reached the end of the archive</p>
              )}
            </div>
          )}
        </section>

        <section className="relative overflow-hidden rounded-[2rem] bg-ink px-6 py-12 md:px-14 md:py-16 text-paper">
          <div className="pointer-events-none absolute -right-8 -top-20 select-none font-display text-[18rem] font-bold leading-none text-white/[0.04]">
            ಅ
          </div>
          <div className="relative flex flex-col gap-8 md:flex-row md:items-end md:justify-between">
            <div className="max-w-xl space-y-3">
              <p className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.18em] text-ochre">
                <Globe size={14} />
                Museum Access
              </p>
              <p className="display text-3xl md:text-4xl text-paper">
                Spotted a sign worth saving? <em className="text-ochre">Add it to the archive.</em>
              </p>
              <p className="text-sm text-paper/60">
                Browse the complete archive to discover documented typographic
                stories from the city.
              </p>
            </div>
            <div className="flex flex-wrap gap-3">
              <button onClick={() => navigate("/contribute")} className="btn-primary">
                <Plus size={16} />
                Add Discovery
              </button>
              <button
                onClick={() => navigate("/map")}
                className="btn border border-white/20 text-paper hover:bg-white/10"
              >
                Explore the map
                <ArrowRight size={16} />
              </button>
            </div>
          </div>
        </section>
      </div>
    </>
  );
};

const GalleryCard: React.FC<{
  page: ZinePageData;
  isNew: boolean;
  onOpen: () => void;
  onExpand: () => void;
  onContributorClick?: () => void;
  onDelete?: () => void;
}> = ({ page, isNew, onOpen, onExpand, onContributorClick, onDelete }) => (
  <article className="group">
    <div className="relative overflow-hidden rounded-2xl bg-line">
      <button onClick={onOpen} className="block w-full rounded-none" aria-label={`Open ${page.title}`}>
        <img
          src={page.thumbnail || page.image}
          className="w-full object-cover transition-transform duration-700 group-hover:scale-[1.04]"
          alt={page.title}
          loading="lazy"
        />
        <span className="pointer-events-none absolute inset-0 bg-gradient-to-t from-black/60 via-black/0 to-black/0 opacity-0 transition-opacity duration-300 group-hover:opacity-100" />
        <span className="pointer-events-none absolute bottom-3 left-3 flex items-center gap-3 text-xs font-medium text-white opacity-0 transition-opacity duration-300 group-hover:opacity-100">
          <span className="flex items-center gap-1">
            <Heart size={13} /> {page.likes_count || 0}
          </span>
          <span className="flex items-center gap-1">
            <MessageCircle size={13} /> {page.comments_count || 0}
          </span>
        </span>
      </button>
      {isNew && (
        <span className="absolute left-3 top-3 rounded-full bg-terracotta px-2 py-0.5 text-[11px] font-semibold text-white">
          New
        </span>
      )}
      <div className="absolute right-2 top-2 flex gap-1.5 opacity-100 transition-opacity md:opacity-0 md:group-hover:opacity-100 md:focus-within:opacity-100">
        <button
          onClick={onExpand}
          className="flex h-8 w-8 items-center justify-center rounded-full bg-white/90 text-ink shadow-soft backdrop-blur hover:bg-white"
          aria-label="View full size"
          title="View full size"
        >
          <Maximize2 size={14} />
        </button>
        {onDelete && (
          <button
            onClick={onDelete}
            className="flex h-8 w-8 items-center justify-center rounded-full bg-white/90 text-red-600 shadow-soft backdrop-blur hover:bg-white"
            aria-label="Delete upload"
            title="Delete upload"
          >
            <Trash2 size={14} />
          </button>
        )}
      </div>
    </div>
    <div className="mt-3 flex items-start justify-between gap-3 px-0.5">
      <div className="min-w-0">
        <button
          onClick={onOpen}
          className="block max-w-full truncate text-left font-semibold leading-snug hover:text-terracotta transition-colors"
        >
          {page.title}
        </button>
        <p className="mt-0.5 text-xs text-muted truncate">
          {page.location}
          {page.contributorName && (
            <>
              {" · "}
              {onContributorClick ? (
                <button onClick={onContributorClick} className="hover:text-ink hover:underline">
                  @{page.contributorName}
                </button>
              ) : (
                <>@{page.contributorName}</>
              )}
            </>
          )}
        </p>
      </div>
      {(page.ml_script || page.vibe) && (
        <span className="chip shrink-0">{page.ml_script || page.vibe}</span>
      )}
    </div>
  </article>
);

export default ExplorePage;
