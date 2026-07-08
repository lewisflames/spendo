/**
 * Fetches every row of a paginated backend list by looping pages.
 * The backend caps page size, so a single request would silently
 * truncate large exports.
 */
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
  }
  return all;
}
