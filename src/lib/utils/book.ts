/** Tauri's save dialog doesn't force the filtered extension — append it. */
export function ensureSpendoExtension(path: string): string {
  return /\.[A-Za-z0-9]+$/.test(path) ? path : `${path}.spendo`;
}

/** Same behavior for CSV export defaults. */
export function ensureCsvExtension(path: string): string {
  return /\.[A-Za-z0-9]+$/.test(path) ? path : `${path}.csv`;
}

/** Same behavior for GnuCash export defaults. */
export function ensureGnuCashExtension(path: string): string {
  return /\.[A-Za-z0-9]+$/.test(path) ? path : `${path}.gnucash`;
}

/** Display name for a book path: basename minus the `.spendo` extension. */
export function bookDisplayName(path: string | null): string {
  if (!path) return 'No book open';
  const base = path.split(/[\\/]/).pop() ?? path;
  return base.replace(/\.spendo$/i, '') || base;
}

/** Parent folder of a book path for compact secondary text, or `null` when
 * the file sits at the filesystem root. */
export function bookFolder(path: string): string | null {
  const idx = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  if (idx <= 0) return null;
  return path.slice(0, idx);
}
