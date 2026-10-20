/** Pure candidate-list helpers for the reconcile page (tested, no Svelte state). */
import type { ReconcileCandidate } from '$lib/tauri';

/** Half-cent tolerance for amount matches, as in the statement math. */
const AMOUNT_EPS = 0.005;

export type CandidateSortKey = 'date' | 'description' | 'amount';

export interface CandidateFilter {
  /** Free-text query over description, number and memo (case-insensitive). */
  query: string;
  /** `query` parsed as an amount, so typing a figure also matches amounts. */
  queryAmount: number | null;
  /** Exact-amount filter (e.g. "find the difference"). */
  amountFilter: number | null;
  sortBy: CandidateSortKey;
  sortDir: 'asc' | 'desc';
}

/** Visible candidates in display order; date ties break on split guid. */
export function filterAndSortCandidates(
  candidates: readonly ReconcileCandidate[],
  { query, queryAmount, amountFilter, sortBy, sortDir }: CandidateFilter
): ReconcileCandidate[] {
  const q = query.trim().toLowerCase();
  const sorted = candidates.filter((c) => {
    if (amountFilter !== null && Math.abs(c.amount - amountFilter) > AMOUNT_EPS) return false;
    if (!q) return true;
    const textMatch = [c.description, c.num, c.memo].some((f) => f?.toLowerCase().includes(q));
    const amountMatch = queryAmount !== null && Math.abs(c.amount - queryAmount) <= AMOUNT_EPS;
    return textMatch || amountMatch;
  });
  const dir = sortDir === 'asc' ? 1 : -1;
  sorted.sort((a, b) => {
    if (sortBy === 'amount') return (a.amount - b.amount || 0) * dir;
    if (sortBy === 'description')
      return (
        (a.description ?? '').localeCompare(b.description ?? '', undefined, {
          sensitivity: 'base'
        }) * dir
      );
    if (a.postDate !== b.postDate) return (a.postDate < b.postDate ? -1 : 1) * dir;
    return (a.splitGuid < b.splitGuid ? -1 : 1) * dir;
  });
  return sorted;
}

/** Reconcile-state changes needed to make the stored states match `checked`. */
export function pendingReconcileUpdates(
  candidates: readonly ReconcileCandidate[],
  checked: Pick<Set<string>, 'has'>
): { splitGuid: string; state: string }[] {
  return candidates
    .filter((c) => (checked.has(c.splitGuid) ? 'c' : 'n') !== c.reconcileState)
    .map((c) => ({ splitGuid: c.splitGuid, state: checked.has(c.splitGuid) ? 'c' : 'n' }));
}
