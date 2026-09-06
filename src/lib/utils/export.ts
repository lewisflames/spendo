import { downloadCsv } from './csv';

/**
 * Fetches every row of a paginated backend list by looping pages.
 * The backend caps page size, so a single request would silently
 * truncate large exports.
 */
/** Hard cap on paginated export rounds (500/page x 10k pages is far beyond any real book). */
const MAX_EXPORT_PAGES = 10_000;

/** Backend export page size (matches the largest page the backend serves). */
export const EXPORT_PAGE_SIZE = 500;

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

/**
 * Paginated-list CSV export: loops every page, maps rows to string cells
 * and downloads. Errors propagate for the caller's toast.
 */
export async function exportPaginatedCsv<T>(opts: {
  filename: string;
  headers: string[];
  fetchPage: (page: number, pageSize: number) => Promise<{ rows: T[]; total: number }>;
  mapRow: (item: T) => string[];
  pageSize?: number;
}): Promise<void> {
  const all = await fetchAllRows<T>(async (pageIdx) => {
    const size = opts.pageSize ?? EXPORT_PAGE_SIZE;
    return opts.fetchPage(pageIdx, size);
  });
  downloadCsv(opts.filename, opts.headers, all.map(opts.mapRow));
}
