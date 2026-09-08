import { describe, expect, it } from 'vitest';
import type { AccountNode } from '$lib/tauri';
import { flattenAccountsForPicker, subtreeGuids } from './account-picker-utils';

function node(partial: Partial<AccountNode> & { guid: string }): AccountNode {
  return {
    name: partial.guid,
    accountType: 'EXPENSE',
    code: null,
    description: null,
    placeholder: false,
    commodity: null,
    commodityGuid: null,
    balance: 0,
    txCount: 0,
    children: [],
    ...partial
  };
}

const trees = [
  node({
    guid: 'expenses',
    name: 'Expenses',
    placeholder: true,
    children: [
      node({ guid: 'food', name: 'Food', description: 'Groceries' }),
      node({ guid: 'savings', name: 'Savings' })
    ]
  })
];

describe('flattenAccountsForPicker', () => {
  it('labels options with full display paths', () => {
    const opts = flattenAccountsForPicker(trees);
    expect(opts.map((o) => o.label)).toEqual([
      'Expenses',
      'Expenses › Groceries',
      'Expenses › Savings'
    ]);
  });

  it('optionally hides placeholders', () => {
    expect(flattenAccountsForPicker(trees, { hidePlaceholders: true }).map((o) => o.guid)).toEqual([
      'food',
      'savings'
    ]);
  });

  it('excludes given guids but still descends into their children', () => {
    const opts = flattenAccountsForPicker(trees, { excludeGuids: ['expenses'] });
    expect(opts.map((o) => o.guid)).toEqual(['food', 'savings']);
  });
});

describe('subtreeGuids', () => {
  it('collects a node plus all descendants', () => {
    expect(subtreeGuids(trees, 'expenses')).toEqual(new Set(['expenses', 'food', 'savings']));
    expect(subtreeGuids(trees, 'food')).toEqual(new Set(['food']));
  });

  it('falls back to the guid alone when missing', () => {
    expect(subtreeGuids(trees, 'nope')).toEqual(new Set(['nope']));
  });
});
