/** Pure statement math for the reconcile page (tested, no Svelte state). */

/** Half-cent tolerance matching the backend `BALANCE_EPS`. */
export const RECONCILE_EPS = 0.005;

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
