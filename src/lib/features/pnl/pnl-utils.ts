/** Change pct, kept in sync with the backend `pnl_change_pct` (reports.rs):
 * null when both zero, otherwise relative change; a new account with no
 * prior balance reports 100%. */
export function changePct(current: number, previous: number): number | null {
  if (previous !== 0) return ((current - previous) / previous) * 100;
  if (current !== 0) return 100;
  return null;
}

/** Text color for a variance amount: green when "good", red otherwise. */
export function varianceColor(value: number, positiveGood: boolean): string {
  if (positiveGood) return value >= 0 ? 'text-success' : 'text-destructive';
  return value >= 0 ? 'text-destructive' : 'text-success';
}
