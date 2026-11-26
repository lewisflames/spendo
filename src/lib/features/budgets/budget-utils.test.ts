import { describe, expect, it } from 'vitest';
import { toUnits as u } from '$lib/utils/units';
import { ZERO_UNITS } from '$lib/utils/units';
import type { AccountNode, BudgetActualRow } from '$lib/tauri';
import {
  accountNameMap,
  budgetAmountError,
  accountPathMap,
  budgetTotals,
  budgetYearBounds,
  clampYear,
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
    balance: ZERO_UNITS,
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
    accountDescription: null,
    accountCode: null,
    accountType: 'EXPENSE',
    budgetAmount: u(0),
    actualAmount: u(0),
    remaining: u(0),
    percentage: 0,
    rolloverBalance: u(0),
    expectedYtd: u(0),
    pacingPercentage: 0,
    delta: u(0),
    elapsedMonths: 6,
    effectiveTotal: u(0),
    ...partial
  };
}

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
      name: 'Expenses',
      description: 'Everyday spending',
      children: [node({ guid: 'food', name: 'Food', description: 'Meals' })]
    })
  ];

  it('builds full paths from account names', () => {
    expect(accountPathMap(trees)).toEqual({
      exp: ['Expenses'],
      food: ['Expenses', 'Food']
    });
  });

  it('maps account names by guid', () => {
    expect(accountNameMap(trees)).toEqual({ exp: 'Expenses', food: 'Food' });
  });

  it('returns empty maps without trees', () => {
    expect(accountPathMap(undefined)).toEqual({});
    expect(accountNameMap(undefined)).toEqual({});
  });
});

describe('budgetAmountError', () => {
  it('accepts positive whole-cent amounts only', () => {
    expect(budgetAmountError(1200)).toBe('');
    expect(budgetAmountError(0.01)).toBe('');
    expect(budgetAmountError(0)).toBe('Enter an amount above zero');
    expect(budgetAmountError(Number.NaN)).toBe('Enter an amount above zero');
    expect(budgetAmountError(12.345)).toBe('Budgets can’t go below a cent');
  });
});

describe('budgetTotals', () => {
  it('sums every amount field across rows', () => {
    const totals = budgetTotals([
      budgetRow({
        budgetAmount: u(1200),
        rolloverBalance: u(300),
        actualAmount: u(400),
        expectedYtd: u(500),
        delta: u(100),
        remaining: u(1100),
        elapsedMonths: 6
      }),
      budgetRow({
        budgetAmount: u(600),
        rolloverBalance: u(-100),
        actualAmount: u(700),
        expectedYtd: u(350),
        delta: u(-350),
        remaining: u(-200),
        elapsedMonths: 6
      })
    ]);
    expect(totals).toEqual({
      budgetAmount: u(1800),
      monthlyAmount: u(150),
      rolloverBalance: u(200),
      actualAmount: u(1100),
      expectedYtd: u(850),
      delta: u(-250),
      remaining: u(900),
      pacingPercentage: (1100 / 850) * 100,
      elapsedMonths: 6
    });
  });

  it('derives monthlyAmount from the annual amount', () => {
    const totals = budgetTotals([budgetRow({ budgetAmount: u(1000) })]);
    // A twelfth, rounded to the unit.
    expect(totals.monthlyAmount).toBe(u(1000 / 12));
  });

  it('weights pacing by aggregate expected, not per-row percentages', () => {
    // A large underspent budget dominates a small overspent one.
    const totals = budgetTotals([
      budgetRow({ actualAmount: u(100), expectedYtd: u(1000), pacingPercentage: 10 }),
      budgetRow({ actualAmount: u(200), expectedYtd: u(200), pacingPercentage: 100 })
    ]);
    expect(totals.pacingPercentage).toBeCloseTo((300 / 1200) * 100);
  });

  it('clamps negative spent and zero expected to 0 pacing', () => {
    expect(
      budgetTotals([budgetRow({ actualAmount: u(-50), expectedYtd: u(100) })]).pacingPercentage
    ).toBe(0);
    expect(
      budgetTotals([budgetRow({ actualAmount: u(50), expectedYtd: u(0) })]).pacingPercentage
    ).toBe(0);
    expect(
      budgetTotals([budgetRow({ actualAmount: u(50), expectedYtd: u(-100) })]).pacingPercentage
    ).toBe(0);
  });

  it('returns zeroed totals for an empty selection', () => {
    expect(budgetTotals([])).toEqual({
      budgetAmount: u(0),
      monthlyAmount: u(0),
      rolloverBalance: u(0),
      actualAmount: u(0),
      expectedYtd: u(0),
      delta: u(0),
      remaining: u(0),
      pacingPercentage: 0,
      elapsedMonths: 0
    });
  });
});
