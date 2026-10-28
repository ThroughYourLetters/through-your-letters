import { keepPreviousData, useInfiniteQuery } from "@tanstack/react-query";
import { webLimits } from "../lib/runtimeConfig";
import { api } from "../lib/api";
import { Lettering } from "../types";

const PAGE_SIZE = webLimits.gallery_page_size;

interface GalleryPage {
  letterings: Lettering[];
  total: number;
  limit?: number;
}

export function useInfiniteGallery(
  cityId?: string | null,
  script?: string | null,
  style?: string | null,
  sortBy?: string | null,
) {
  return useInfiniteQuery<GalleryPage>({
    queryKey: [
      "letterings-infinite",
      cityId ?? "all",
      script ?? "",
      style ?? "",
      sortBy ?? "",
    ],
    queryFn: async ({ pageParam }) => {
      const offset = pageParam as number;
      return api.getGallery({
        limit: PAGE_SIZE,
        offset,
        cityId,
        script,
        style,
        sortBy,
      });
    },
    initialPageParam: 0,
    // Keep the current grid on screen while a new filter loads
    placeholderData: keepPreviousData,
    getNextPageParam: (lastPage, allPages) => {
      const totalLoaded = allPages.reduce(
        (sum, page) => sum + page.letterings.length,
        0,
      );
      if (totalLoaded >= lastPage.total || lastPage.letterings.length < (lastPage.limit ?? PAGE_SIZE)) return undefined;
      return totalLoaded;
    },
  });
}
