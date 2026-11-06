type Page = {
  items: unknown[];
  total: number;
  limit: number;
  next_cursor?: string | null;
};

/** Older API instances return offset pages during rolling deployments. */
export function nextAccountPage(last: Page, pages: Page[]) {
  if (last.next_cursor !== undefined) return last.next_cursor ?? undefined;
  const loaded = pages.reduce((count, page) => count + page.items.length, 0);
  return loaded < last.total && last.items.length === last.limit
    ? loaded
    : undefined;
}
