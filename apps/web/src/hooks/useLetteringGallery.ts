import { keepPreviousData, useInfiniteQuery } from "@tanstack/react-query";
import { webLimits } from "../lib/runtimeConfig";
import { api } from "../lib/api";

const PAGE_SIZE = webLimits.gallery_page_size;

export function useInfiniteGallery(
  cityId?: string | null,
  script?: string | null,
  style?: string | null,
  sortBy?: string | null,
) {
  return useInfiniteQuery({
    queryKey: [
      "letterings-infinite",
      cityId ?? "all",
      script ?? "",
      style ?? "",
      sortBy ?? "",
    ],
    queryFn: ({ pageParam }) =>
      api.getGallery({
        limit: PAGE_SIZE,
        ...(typeof pageParam === "string"
          ? { after: pageParam }
          : { offset: pageParam }),
        cityId,
        script,
        style,
        sortBy,
      }),
    initialPageParam: 0 as number | string,
    // Keep the current grid on screen while a new filter loads.
    placeholderData: keepPreviousData,
    getNextPageParam: (lastPage, allPages): number | string | undefined => {
      if (lastPage.next_cursor !== undefined)
        return lastPage.next_cursor ?? undefined;
      // During rolling deployments an older API may still return offset pages.
      const loaded = allPages.reduce(
        (sum, page) => sum + page.letterings.length,
        0,
      );
      if (
        loaded >= lastPage.total ||
        lastPage.letterings.length < (lastPage.limit ?? PAGE_SIZE)
      )
        return undefined;
      return loaded;
    },
  });
}
