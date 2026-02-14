import { useInfiniteQuery } from "@tanstack/react-query";
import { api } from "../lib/api";
import { Lettering } from "../types";

const PAGE_SIZE = 30;

interface GalleryPage {
  letterings: Lettering[];
  total: number;
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
    getNextPageParam: (lastPage, allPages) => {
      const totalLoaded = allPages.reduce(
        (sum, page) => sum + page.letterings.length,
        0,
      );
      if (lastPage.letterings.length < PAGE_SIZE) return undefined;
      return totalLoaded;
    },
  });
}
