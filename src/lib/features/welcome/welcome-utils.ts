/** Mirrors the open dialog’s filter: only the app’s own book files qualify. */
const BOOK_EXTENSIONS = ['.spendo', '.db', '.sqlite'];

export function isSpendoBookPath(path: string): boolean {
  const lower = path.toLowerCase();
  return BOOK_EXTENSIONS.some((ext) => lower.endsWith(ext));
}

/** A book the welcome screen offers to reopen; `openedAt` is unix seconds. */
export interface RecentBook {
  path: string;
  openedAt: number;
}

export const MAX_RECENT_BOOKS = 8;

/** Stored recents, keeping only well-formed entries (newest first, deduped). */
export function parseRecentBooks(raw: unknown): RecentBook[] {
  if (!Array.isArray(raw)) return [];
  const seen = new Set<string>();
  const books: RecentBook[] = [];
  for (const item of raw) {
    if (typeof item !== 'object' || item === null) continue;
    const { path, openedAt } = item as Record<string, unknown>;
    if (typeof path !== 'string' || !path || typeof openedAt !== 'number') continue;
    if (!Number.isFinite(openedAt) || seen.has(path)) continue;
    seen.add(path);
    books.push({ path, openedAt });
  }
  return books.sort((a, b) => b.openedAt - a.openedAt).slice(0, MAX_RECENT_BOOKS);
}

/** Move `path` to the front with a fresh timestamp, capped at the max. */
export function withRecentBook(books: RecentBook[], path: string, now: number): RecentBook[] {
  return [{ path, openedAt: now }, ...books.filter((b) => b.path !== path)].slice(
    0,
    MAX_RECENT_BOOKS
  );
}
