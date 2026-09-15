/** Mirrors the open dialog's filter: only the app's own book files qualify. */
const BOOK_EXTENSIONS = ['.spendo', '.db', '.sqlite'];

export function isSpendoBookPath(path: string): boolean {
  const lower = path.toLowerCase();
  return BOOK_EXTENSIONS.some((ext) => lower.endsWith(ext));
}
