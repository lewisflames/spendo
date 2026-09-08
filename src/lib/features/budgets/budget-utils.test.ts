import { describe, expect, it } from 'vitest';
import type { AccountNode } from '$lib/tauri';
import {
  accountNameMap,
  accountPathMap,
  budgetYearBounds,
  buildYearOptions,
  clampYear,
  fallbackYearOptions,
  flattenExpenseAccounts
} from './budget-utils';

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

describe('buildYearOptions', () => {
  it('builds a contiguous string-valued range', () => {
    expect(buildYearOptions(2022, 2024)).toEqual([
      { value: '2022', label: '2022' },
      { value: '2023', label: '2023' },
      { value: '2024', label: '2024' }
    ]);
  });
});

describe('fallbackYearOptions', () => {
  it('centers radius + 6 entries ending above the center year', () => {
    const opts = fallbackYearOptions(2024, 2);
    expect(opts.length).toBe(8);
    expect(opts[0].value).toBe('2022');
    expect(opts[opts.length - 1].value).toBe('2029');
  });
});

describe('clampYear', () => {
  it('clamps to non-null bounds, passes through otherwise', () => {
    expect(clampYear(2020, 2022, 2024)).toBe(2022);
    expect(clampYear(2026, 2022, 2024)).toBe(2024);
    expect(clampYear(2023, 2022, 2024)).toBe(2023);
    expect(clampYear(1999, null, null)).toBe(1999);
    expect(clampYear(1999, null, 2024)).toBe(1999);
  });
});

describe('budgetYearBounds', () => {
  it('derives bounds from the transaction window', () => {
    expect(budgetYearBounds({ earliestDate: '2021-03-01', latestDate: '2023-11-30' })).toEqual({
      minYear: 2021,
      maxYear: 2023
    });
  });

  it('extends max by one with plusOne', () => {
    expect(
      budgetYearBounds({ earliestDate: '2021-03-01', latestDate: '2023-11-30' }, true)
    ).toEqual({ minYear: 2021, maxYear: 2024 });
  });

  it('returns nulls without a date range', () => {
    expect(budgetYearBounds(null)).toEqual({ minYear: null, maxYear: null });
  });
});

describe('flattenExpenseAccounts', () => {
  it('collects non-placeholder expense accounts recursively', () => {
    const trees = [
      node({
        guid: 'food',
        description: 'Food',
        children: [node({ guid: 'groceries' }), node({ guid: 'dining', placeholder: true })]
      }),
      node({ guid: 'checking', accountType: 'BANK' })
    ];
    expect(flattenExpenseAccounts(trees).map((a) => a.guid)).toEqual(['food', 'groceries']);
  });
});

describe('accountPathMap / accountNameMap', () => {
  const trees = [
    node({
      guid: 'exp',
      description: 'Expenses',
      children: [node({ guid: 'food', name: 'Food', description: 'Meals' })]
    })
  ];

  it('prefers descriptions and builds full paths', () => {
    expect(accountPathMap(trees)).toEqual({
      exp: ['Expenses'],
      food: ['Expenses', 'Meals']
    });
  });

  it('maps display names by guid', () => {
    expect(accountNameMap(trees)).toEqual({ exp: 'Expenses', food: 'Meals' });
  });

  it('returns empty maps without trees', () => {
    expect(accountPathMap(undefined)).toEqual({});
    expect(accountNameMap(undefined)).toEqual({});
  });
});
