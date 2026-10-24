/** Change pct, kept in sync with the backend `pnl_change_pct` (reports.rs):
 * null when both zero, otherwise relative change; a new account with no
 * prior balance reports 100%. */
export function changePct(current: number, previous: number): number | null {
  if (previous !== 0) return ((current - previous) / previous) * 100;
  if (current !== 0) return 100;
  return null;
}

/** Share of income kept (net ÷ income, 0–100 scale); null without positive
 * income, where a rate is meaningless. Negative when spending exceeds income. */
export function savingsRate(income: number, net: number): number | null {
  return income > 0 ? (net / income) * 100 : null;
}
