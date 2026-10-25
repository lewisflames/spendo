import { describe, expect, it } from 'vitest';
import {
  bulkResultMessage,
  pageSelectionState,
  rangeRows,
  selectionAccounts,
  selectionTags
} from './bulk-edit';

const rows = [{ guid: 'a' }, { guid: 'b' }, { guid: 'c' }, { guid: 'd' }];

describe('bulk edit selection', () => {
  it('reports the header checkbox state for the page', () => {
    expect(pageSelectionState(new Map(), [])).toBe('none');
    expect(pageSelectionState(new Map(), rows)).toBe('none');
    expect(pageSelectionState(new Map([['b', 1]]), rows)).toBe('some');
    expect(pageSelectionState(new Map(rows.map((r) => [r.guid, r])), rows)).toBe('all');
  });

  it('selects a contiguous range in either direction', () => {
    expect(rangeRows(rows, 'b', 'd').map((r) => r.guid)).toEqual(['b', 'c', 'd']);
    expect(rangeRows(rows, 'd', 'b').map((r) => r.guid)).toEqual(['b', 'c', 'd']);
  });

  it('falls back to the clicked row without an anchor on the page', () => {
    expect(rangeRows(rows, null, 'c').map((r) => r.guid)).toEqual(['c']);
    expect(rangeRows(rows, 'elsewhere', 'c').map((r) => r.guid)).toEqual(['c']);
    expect(rangeRows(rows, 'a', 'missing')).toEqual([]);
  });

  it('ranks the accounts the selection posts to', () => {
    const split = (accountGuid: string, accountName: string) => ({
      guid: `${accountGuid}-split`,
      accountGuid,
      accountName,
      amount: 1
    });
    const accounts = selectionAccounts([
      { guid: 't1', splits: [split('bank', 'Bank'), split('food', 'Food')] },
      // Two splits in the same account count once per transaction.
      { guid: 't2', splits: [split('bank', 'Bank'), split('fun', 'Fun'), split('fun', 'Fun')] },
      { guid: 't3', splits: [split('bank', 'Bank'), split('food', 'Food')] }
    ]);
    expect(accounts).toEqual([
      { guid: 'bank', count: 3 },
      { guid: 'food', count: 2 },
      { guid: 'fun', count: 1 }
    ]);
  });
});

describe('selectionTags', () => {
  it('lists the tags on the selection, most used first', () => {
    const trip = { guid: 't', name: 'trip' };
    const work = { guid: 'w', name: 'work' };
    const tax = { guid: 'x', name: 'deductible' };
    expect(
      selectionTags([{ tags: [work] }, { tags: [trip, work] }, { tags: [tax, trip] }, { tags: [] }])
    ).toEqual([trip, work, tax]);
  });
});

describe('bulkResultMessage', () => {
  it('words tag changes', () => {
    expect(bulkResultMessage('addTag', { changed: 2, skippedLocked: 0 })).toBe(
      'Tagged 2 transactions'
    );
    expect(bulkResultMessage('removeTag', { changed: 0, skippedLocked: 0 })).toBe(
      'Nothing to remove. None of them have that tag'
    );
  });

  it('summarizes what changed', () => {
    expect(bulkResultMessage('delete', { changed: 3, skippedLocked: 0 })).toBe(
      'Deleted 3 transactions'
    );
    expect(bulkResultMessage('setDescription', { changed: 1, skippedLocked: 0 })).toBe(
      'Renamed 1 transaction'
    );
  });

  it('mentions locked transactions it left alone', () => {
    expect(bulkResultMessage('moveSplits', { changed: 2, skippedLocked: 1 })).toBe(
      'Moved 2 transactions · 1 cleared or reconciled transaction left as is'
    );
    expect(bulkResultMessage('delete', { changed: 0, skippedLocked: 2 })).toBe(
      'Nothing changed. 2 cleared or reconciled transactions left as is'
    );
  });

  it('explains a move that matched no splits', () => {
    expect(bulkResultMessage('moveSplits', { changed: 0, skippedLocked: 0 })).toBe(
      'Nothing to move. None of them use that account'
    );
  });
});
