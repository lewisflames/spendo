import { describe, expect, it } from 'vitest';
import { impactLabel, sourceLabel, type LabelFields } from './history-labels';

const entry = (over: Partial<LabelFields> = {}): LabelFields => ({
  kind: 'tx_create',
  source: 'manual',
  transactionsAffected: 0,
  accountsAffected: 0,
  ...over
});

describe('sourceLabel', () => {
  it('labels imports by format', () => {
    expect(sourceLabel(entry({ kind: 'import', source: 'import' }))).toBe('Import');
    expect(sourceLabel(entry({ kind: 'import', source: 'csv' }))).toBe('CSV');
  });

  it('distinguishes scheduled posts, reconciles and undos from manual edits', () => {
    expect(sourceLabel(entry({ source: 'schedule' }))).toBe('Scheduled');
    expect(sourceLabel(entry({ kind: 'reconcile' }))).toBe('Reconcile');
    expect(sourceLabel(entry({ kind: 'revert', source: 'system' }))).toBe('Undo');
    expect(sourceLabel(entry({ kind: 'redo', source: 'system' }))).toBe('Redo');
    expect(sourceLabel(entry({ kind: 'account_update' }))).toBe('Manual');
  });
});

describe('impactLabel', () => {
  it('summarizes batch changes', () => {
    expect(
      impactLabel(entry({ kind: 'import', transactionsAffected: 12, accountsAffected: 2 }))
    ).toBe('12 transactions · 2 accounts');
    expect(impactLabel(entry({ kind: 'import', transactionsAffected: 1 }))).toBe('1 transaction');
    expect(
      impactLabel(entry({ kind: 'reconcile', transactionsAffected: 5, accountsAffected: 1 }))
    ).toBe('5 transactions · 1 account');
  });

  it('hides counts for single-entity edits and empty batches', () => {
    expect(impactLabel(entry({ kind: 'tx_update', transactionsAffected: 1 }))).toBeNull();
    expect(impactLabel(entry({ kind: 'account_create', accountsAffected: 1 }))).toBeNull();
    expect(impactLabel(entry({ kind: 'import' }))).toBeNull();
  });
});
