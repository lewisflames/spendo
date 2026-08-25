/**
 * Fetches every row of a paginated backend list by looping pages.
 * The backend caps page size, so a single request would silently
 * truncate large exports.
 */
/** Hard cap on paginated export rounds (500/page x 10k pages is far beyond any real book). */
const MAX_EXPORT_PAGES = 10_000;

export async function fetchAllRows<T>(
  fetchPage: (page: number) => Promise<{ rows: T[]; total: number }>
): Promise<T[]> {
  const all: T[] = [];
  let pageIdx = 1;
  for (;;) {
    const { rows, total } = await fetchPage(pageIdx);
    all.push(...rows);
    if (all.length >= total || rows.length === 0) break;
    pageIdx++;
    // Guard against a backend misreporting `total` and looping forever.
    if (pageIdx > MAX_EXPORT_PAGES) break;
  }
  return all;
}
