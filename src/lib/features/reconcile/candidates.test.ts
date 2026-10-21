import { describe, expect, it } from 'vitest';
import type { ReconcileCandidate } from '$lib/tauri';
import {
  filterAndSortCandidates,
  pendingReconcileUpdates,
  type CandidateFilter
} from './candidates';

function candidate(
  splitGuid: string,
  postDate: string,
  amount: number,
  extra: Partial<ReconcileCandidate> = {}
): ReconcileCandidate {
  return {
    splitGuid,
    txGuid: `tx-${splitGuid}`,
    postDate,
    description: null,
    amount,
    reconcileState: 'n',
    counterAccounts: [],
    ...extra
  };
}

const rows = [
  candidate('b', '2026-01-02', -40, { description: 'Groceries', memo: 'weekly' }),
  candidate('a', '2026-01-02', 12.5, { description: 'refund' }),
  candidate('c', '2026-01-01', 1000, { description: 'Salary', num: '1001' })
];

const base: CandidateFilter = {
  query: '',
  queryAmount: null,
  amountFilter: null,
  sortBy: 'date',
  sortDir: 'asc'
};

const guids = (list: ReconcileCandidate[]) => list.map((c) => c.splitGuid);

describe('filterAndSortCandidates', () => {
  it('sorts by date then split guid, in either direction', () => {
    expect(guids(filterAndSortCandidates(rows, base))).toEqual(['c', 'a', 'b']);
    expect(guids(filterAndSortCandidates(rows, { ...base, sortDir: 'desc' }))).toEqual([
      'b',
      'a',
      'c'
    ]);
  });

  it('sorts by amount and case-insensitive description', () => {
    expect(guids(filterAndSortCandidates(rows, { ...base, sortBy: 'amount' }))).toEqual([
      'b',
      'a',
      'c'
    ]);
    expect(guids(filterAndSortCandidates(rows, { ...base, sortBy: 'description' }))).toEqual([
      'b',
      'a',
      'c'
    ]);
  });

  it('matches text in description, number or memo, or a typed amount', () => {
    expect(guids(filterAndSortCandidates(rows, { ...base, query: 'WEEK' }))).toEqual(['b']);
    expect(guids(filterAndSortCandidates(rows, { ...base, query: '1001' }))).toEqual(['c']);
    expect(
      guids(filterAndSortCandidates(rows, { ...base, query: '12.50', queryAmount: 12.5 }))
    ).toEqual(['a']);
  });

  it('applies the exact-amount filter within half a cent', () => {
    expect(guids(filterAndSortCandidates(rows, { ...base, amountFilter: -40.004 }))).toEqual(['b']);
    expect(filterAndSortCandidates(rows, { ...base, amountFilter: -40.01 })).toEqual([]);
  });

  it('does not reorder its input', () => {
    filterAndSortCandidates(rows, base);
    expect(guids(rows)).toEqual(['b', 'a', 'c']);
  });
});

describe('pendingReconcileUpdates', () => {
  it('lists only rows whose checked state differs from the stored state', () => {
    const stored = [
      candidate('x', '2026-01-01', 1, { reconcileState: 'c' }),
      candidate('y', '2026-01-01', 2),
      candidate('z', '2026-01-01', 3, { reconcileState: 'c' })
    ];
    expect(pendingReconcileUpdates(stored, new Set(['x', 'y']))).toEqual([
      { splitGuid: 'y', state: 'c' },
      { splitGuid: 'z', state: 'n' }
    ]);
    expect(pendingReconcileUpdates(stored, new Set(['x', 'z']))).toEqual([]);
  });
});
