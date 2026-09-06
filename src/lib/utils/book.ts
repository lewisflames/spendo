/** Tauri's save dialog doesn't force the filtered extension — append it. */
export function ensureSpendoExtension(path: string): string {
  return /\.[A-Za-z0-9]+$/.test(path) ? path : `${path}.spendo`;
}
