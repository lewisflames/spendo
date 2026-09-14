/** Pure statement math for the reconcile page (tested, no Svelte state). */

/** Half-cent tolerance matching the backend `BALANCE_EPS`. */
const RECONCILE_EPS = 0.005;

/** Sum of checked candidate amounts (signed display amounts). */
export function clearedTotal(amounts: number[]): number {
  return amounts.reduce((sum, n) => sum + n, 0);
}

/** Expected statement total from the reconciled opening balance + cleared. */
export function targetBalance(startingBalance: number, cleared: number): number {
  return startingBalance + cleared;
}

/** `ending - target`; zero means the statement balances. */
export function statementDifference(endingBalance: number, target: number): number {
  return endingBalance - target;
}

/** Whether the statement balances within half-cent tolerance. */
export function isBalanced(difference: number, eps = RECONCILE_EPS): boolean {
  return Math.abs(difference) <= eps;
}

/**
 * Parse a statement ending-balance input. Accepts plain decimal text with an
 * optional `.`/`,` separator (`1234.56`, `-12,5`); rejects blanks,
 * thousands-grouped text (`1,000.50`) and anything non-numeric so Finish
 * can't silently round a mistyped value. Returns `null` when empty/invalid.
 */
export function parseStatementAmount(raw: string): number | null {
  const text = raw.trim().replace(/\s+/g, '');
  if (text === '') return null;
  const m = /^[+-]?(\d+)(?:([.,])(\d+))?$/.exec(text);
  if (!m) return null;
  // A lone separator with exactly three digits after it is ambiguous
  // (thousands grouping vs. decimal), so reject instead of guessing.
  if (m[2] && m[3].length === 3) return null;
  const n = Number(text.replace(',', '.'));
  return Number.isFinite(n) ? n : null;
}

/** Tri-state of the table header checkbox for the visible rows. */
export function selectionState(
  checked: Pick<Set<string>, 'has'>,
  rows: readonly { splitGuid: string }[]
): 'all' | 'some' | 'none' {
  if (rows.length === 0) return 'none';
  let count = 0;
  for (const r of rows) if (checked.has(r.splitGuid)) count += 1;
  if (count === 0) return 'none';
  return count === rows.length ? 'all' : 'some';
}

/** Whether the reconcile workflow supports an account (mirrors backend `RECONCILABLE_TYPES`). */
export function isReconcilable(accountType: string, placeholder: boolean): boolean {
  if (placeholder) return false;
  return (
    accountType === 'ASSET' ||
    accountType === 'BANK' ||
    accountType === 'CASH' ||
    accountType === 'CURRENCY' ||
    accountType === 'RECEIVABLE' ||
    accountType === 'LIABILITY' ||
    accountType === 'CREDIT' ||
    accountType === 'PAYABLE'
  );
}
