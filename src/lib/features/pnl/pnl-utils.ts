/** Change pct, kept in sync with the backend `pnl_change_pct` (reports.rs):
 * null when both zero, otherwise relative change; a new account with no
 * prior balance reports 100%. */
export function changePct(current: number, previous: number): number | null {
  if (previous !== 0) return ((current - previous) / previous) * 100;
  if (current !== 0) return 100;
  return null;
}
