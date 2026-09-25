import { describe, expect, it } from 'vitest';
import type { AccountNode } from '$lib/tauri';
import { flattenReconcilableAccounts } from './reconcile-accounts';

function node(partial: Partial<AccountNode> & { guid: string; name: string }): AccountNode {
  return {
    accountType: 'BANK',
    code: null,
    description: null,
    placeholder: false,
    commodity: 'EUR',
    commodityGuid: null,
    balance: 0,
    txCount: 0,
    children: [],
    ...partial
  };
}

describe('flattenReconcilableAccounts', () => {
  it('keeps cash-like accounts with leaf display names and full path labels, sorted', () => {
    const trees = [
      node({
        guid: 'assets',
        name: 'Assets',
        accountType: 'ASSET',
        placeholder: true,
        children: [
          node({ guid: 'checking', name: 'Checking', balance: 100, txCount: 3 }),
          node({ guid: 'broker', name: 'Broker', accountType: 'STOCK', balance: 500 })
        ]
      }),
      node({ guid: 'card', name: 'Card', accountType: 'CREDIT', balance: -20, txCount: 1 })
    ];
    const rows = flattenReconcilableAccounts(trees, [
      {
        accountGuid: 'checking',
        unreconciledCount: 2,
        clearedCount: 1,
        unclearedTotal: -30,
        lastReconciledDate: '2024-01-10'
      }
    ]);
    expect(rows.map((r) => r.guid)).toEqual(['checking', 'card']);
    expect(rows.map((r) => r.display)).toEqual(['Checking', 'Card']);
    expect(rows.find((r) => r.guid === 'checking')?.label).toBe('Assets › Checking');
    expect(rows.find((r) => r.guid === 'card')?.label).toBe('Card');
    expect(rows.every((r) => r.parentPath === null)).toBe(true);
    expect(rows.find((r) => r.guid === 'checking')?.stats).toEqual({
      unreconciledCount: 2,
      clearedCount: 1,
      unclearedTotal: -30,
      lastReconciledDate: '2024-01-10'
    });
    // Accounts without a summary row read as pristine.
    expect(rows.find((r) => r.guid === 'card')?.stats).toEqual({
      unreconciledCount: 0,
      clearedCount: 0,
      unclearedTotal: 0,
      lastReconciledDate: null
    });
  });

  it('detects duplicate display names among reconcilable rows only', () => {
    const trees = [
      node({
        guid: 'bank-a',
        name: 'Bank A',
        accountType: 'ASSET',
        placeholder: true,
        children: [node({ guid: 'checking-a', name: 'Checking' })]
      }),
      node({
        guid: 'bank-b',
        name: 'Bank B',
        accountType: 'ASSET',
        placeholder: true,
        children: [node({ guid: 'checking-b', name: 'Checking', accountType: 'STOCK' })]
      })
    ];
    const rows = flattenReconcilableAccounts(trees);
    expect(rows.map((r) => r.guid)).toEqual(['checking-a']);
    expect(rows[0].parentPath).toBeNull();
  });

  it('returns empty for missing trees', () => {
    expect(flattenReconcilableAccounts(null)).toEqual([]);
    expect(flattenReconcilableAccounts(undefined)).toEqual([]);
  });
});
