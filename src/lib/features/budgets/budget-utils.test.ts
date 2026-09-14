import { describe, expect, it } from 'vitest';
import type { AccountNode, BudgetActualRow } from '$lib/tauri';
import {
  accountNameMap,
  accountPathMap,
  budgetTotals,
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

function budgetRow(partial: Partial<BudgetActualRow>): BudgetActualRow {
  return {
    guid: 'budget',
    accountGuid: 'food',
    accountName: 'Food',
    accountDescription: undefined,
    accountType: 'EXPENSE',
    budgetAmount: 0,
    actualAmount: 0,
    remaining: 0,
    percentage: 0,
    rolloverBalance: 0,
    expectedYtd: 0,
    pacingPercentage: 0,
    delta: 0,
    elapsedMonths: 6,
    effectiveTotal: 0,
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

describe('budgetTotals', () => {
  it('sums every amount field across rows', () => {
    const totals = budgetTotals([
      budgetRow({
        budgetAmount: 1200,
        rolloverBalance: 300,
        actualAmount: 400,
        expectedYtd: 500,
        delta: 100,
        remaining: 1100,
        elapsedMonths: 6
      }),
      budgetRow({
        budgetAmount: 600,
        rolloverBalance: -100,
        actualAmount: 700,
        expectedYtd: 350,
        delta: -350,
        remaining: -200,
        elapsedMonths: 6
      })
    ]);
    expect(totals).toEqual({
      budgetAmount: 1800,
      rolloverBalance: 200,
      actualAmount: 1100,
      expectedYtd: 850,
      delta: -250,
      remaining: 900,
      pacingPercentage: (1100 / 850) * 100,
      elapsedMonths: 6
    });
  });

  it('weights pacing by aggregate expected, not per-row percentages', () => {
    // A large underspent budget dominates a small overspent one.
    const totals = budgetTotals([
      budgetRow({ actualAmount: 100, expectedYtd: 1000, pacingPercentage: 10 }),
      budgetRow({ actualAmount: 200, expectedYtd: 200, pacingPercentage: 100 })
    ]);
    expect(totals.pacingPercentage).toBeCloseTo((300 / 1200) * 100);
  });

  it('clamps negative spent and zero expected to 0 pacing', () => {
    expect(
      budgetTotals([budgetRow({ actualAmount: -50, expectedYtd: 100 })]).pacingPercentage
    ).toBe(0);
    expect(budgetTotals([budgetRow({ actualAmount: 50, expectedYtd: 0 })]).pacingPercentage).toBe(
      0
    );
    expect(
      budgetTotals([budgetRow({ actualAmount: 50, expectedYtd: -100 })]).pacingPercentage
    ).toBe(0);
  });

  it('returns zeroed totals for an empty selection', () => {
    expect(budgetTotals([])).toEqual({
      budgetAmount: 0,
      rolloverBalance: 0,
      actualAmount: 0,
      expectedYtd: 0,
      delta: 0,
      remaining: 0,
      pacingPercentage: 0,
      elapsedMonths: 0
    });
  });
});
