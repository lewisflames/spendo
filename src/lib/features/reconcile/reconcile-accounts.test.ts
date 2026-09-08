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
  it('keeps cash-like accounts with full path labels, sorted', () => {
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
    const rows = flattenReconcilableAccounts(trees);
    expect(rows.map((r) => r.guid)).toEqual(['checking', 'card']);
    expect(rows.find((r) => r.guid === 'checking')?.label).toBe('Assets › Checking');
    expect(rows.find((r) => r.guid === 'card')?.label).toBe('Card');
  });

  it('returns empty for missing trees', () => {
    expect(flattenReconcilableAccounts(null)).toEqual([]);
    expect(flattenReconcilableAccounts(undefined)).toEqual([]);
  });
});
