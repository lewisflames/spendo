import { describe, expect, it } from 'vitest';
import { toUnits as u } from '$lib/utils/units';
import type { AccountNode, AccountsResult } from '$lib/tauri';
import { buildBalanceSheet, comparisonDate, subtreeTotal } from './balance-sheet';

function node(partial: Partial<AccountNode> & { guid: string; name: string }): AccountNode {
  return {
    accountType: 'BANK',
    code: null,
    description: null,
    placeholder: false,
    commodity: 'USD',
    commodityGuid: null,
    balance: u(0),
    txCount: 0,
    children: [],
    ...partial
  };
}

function result(partial: Partial<AccountsResult>): AccountsResult {
  return {
    trees: [],
    byType: {},
    totalAccounts: 0,
    reportCurrency: 'USD',
    retainedEarnings: u(0),
    unrealizedGains: u(0),
    balanceCheck: u(0),
    missingFx: [],
    missingPrices: [],
    ...partial
  };
}

describe('subtreeTotal', () => {
  it('uses a placeholder balance as-is (backend already rolls children up)', () => {
    const assets = node({
      guid: 'assets',
      name: 'Assets',
      accountType: 'ASSET',
      placeholder: true,
      balance: u(150),
      children: [
        node({ guid: 'checking', name: 'Checking', balance: u(100) }),
        node({ guid: 'savings', name: 'Savings', balance: u(50) })
      ]
    });
    expect(subtreeTotal(assets)).toBe(u(150));
  });

  it('adds the subtree for non-placeholder parents', () => {
    const parent = node({
      guid: 'parent',
      name: 'Parent',
      balance: u(40),
      children: [
        node({ guid: 'a', name: 'A', balance: u(10) }),
        node({
          guid: 'b',
          name: 'B',
          balance: u(5),
          children: [node({ guid: 'c', name: 'C', balance: u(2) })]
        })
      ]
    });
    expect(subtreeTotal(parent)).toBe(u(57));
  });
});

describe('buildBalanceSheet', () => {
  const data = result({
    trees: [
      node({
        guid: 'assets',
        name: 'Assets',
        accountType: 'ASSET',
        placeholder: true,
        balance: u(1050),
        children: [
          node({ guid: 'checking', name: 'Checking', balance: u(650) }),
          node({ guid: 'brokerage', name: 'Brokerage', accountType: 'STOCK', balance: u(400) })
        ]
      }),
      node({
        guid: 'income',
        name: 'Income',
        accountType: 'INCOME',
        placeholder: true,
        balance: u(500)
      }),
      node({
        guid: 'liabilities',
        name: 'Liabilities',
        accountType: 'LIABILITY',
        placeholder: true,
        balance: u(300),
        children: [node({ guid: 'card', name: 'Card', accountType: 'CREDIT', balance: u(300) })]
      }),
      node({
        guid: 'equity',
        name: 'Equity',
        accountType: 'EQUITY',
        placeholder: true,
        balance: u(200)
      }),
      node({
        guid: 'expenses',
        name: 'Expenses',
        accountType: 'EXPENSE',
        placeholder: true,
        balance: u(50)
      })
    ],
    byType: {
      ASSET: { count: 3, totalBalance: u(1050) },
      LIABILITY: { count: 2, totalBalance: u(300) },
      EQUITY: { count: 1, totalBalance: u(200) },
      INCOME: { count: 1, totalBalance: u(500) },
      EXPENSE: { count: 1, totalBalance: u(50) }
    },
    retainedEarnings: u(450),
    unrealizedGains: u(100)
  });

  it('splits rows into statement sections with depth-first indents', () => {
    const sheet = buildBalanceSheet(data);
    // The top-level placeholder root is the section group: the header names
    // it, so it isn't repeated as a row.
    expect(sheet.assets.rows.map((r) => [r.accountGuid, r.depth, r.amount])).toEqual([
      ['checking', 1, u(650)],
      ['brokerage', 1, u(400)]
    ]);
    expect(sheet.liabilities.rows.map((r) => r.accountGuid)).toEqual(['card']);
    expect(sheet.assets.label).toBe('Assets');
    expect(sheet.liabilities.total).toBe(u(300));
  });

  it('keeps non-placeholder top-level roots and nested placeholders', () => {
    const trees = [
      node({
        guid: 'investments',
        name: 'Investments',
        accountType: 'ASSET',
        balance: u(0),
        children: [
          node({
            guid: 'brokerage',
            name: 'Brokerage',
            accountType: 'ASSET',
            placeholder: true,
            balance: u(400),
            children: [node({ guid: 'vti', name: 'VTI', accountType: 'STOCK', balance: u(400) })]
          })
        ]
      }),
      node({ guid: 'loose', name: 'Loose cash', accountType: 'CASH', balance: u(25) })
    ];
    const sheet = buildBalanceSheet(
      result({ trees, byType: { ASSET: { count: 3, totalBalance: u(425) } } })
    );
    expect(sheet.assets.rows.map((r) => [r.accountGuid, r.depth, r.amount])).toEqual([
      ['investments', 0, u(400)],
      ['brokerage', 1, u(400)],
      ['vti', 2, u(400)],
      ['loose', 0, u(25)]
    ]);
  });

  it('appends retained earnings and unrealized gains to equity', () => {
    const sheet = buildBalanceSheet(data);
    expect(sheet.equity.rows.map((r) => [r.accountGuid, r.name, r.amount])).toEqual([
      ['', 'Retained earnings', u(450)],
      ['', 'Unrealized gains', u(100)]
    ]);
    expect(sheet.equity.total).toBe(u(750));
    // Equity total includes both synthetic lines, so the sheet balances.
    expect(sheet.totalLiabilitiesAndEquity).toBe(u(1050));
    expect(sheet.assets.total).toBe(sheet.totalLiabilitiesAndEquity);
  });

  it('hides a zero unrealized gains line', () => {
    const sheet = buildBalanceSheet({ ...data, unrealizedGains: u(0) });
    expect(sheet.equity.rows.some((r) => r.name === 'Unrealized gains')).toBe(false);
    expect(sheet.equity.total).toBe(u(650));
  });

  it('returns empty sections for an empty book', () => {
    const sheet = buildBalanceSheet(result({}));
    expect(sheet.assets.rows).toEqual([]);
    expect(sheet.liabilities.rows).toEqual([]);
    expect(sheet.equity.rows.map((r) => r.name)).toEqual(['Retained earnings']);
    expect(sheet.totalLiabilitiesAndEquity).toBe(u(0));
  });
});

describe('buildBalanceSheet with a comparison', () => {
  const tree = (balance: number) => [
    node({ guid: 'checking', name: 'Checking', accountType: 'BANK', balance: u(balance) })
  ];

  it('pairs each account with its amount on the comparison date', () => {
    const sheet = buildBalanceSheet(
      result({ trees: tree(300), byType: { ASSET: { count: 1, totalBalance: u(300) } } }),
      result({ trees: tree(100), byType: { ASSET: { count: 1, totalBalance: u(100) } } })
    );
    expect(sheet.assets.rows).toEqual([
      expect.objectContaining({ accountGuid: 'checking', amount: u(300), compareAmount: u(100) })
    ]);
    expect(sheet.assets.compareTotal).toBe(u(100));
  });

  it('keeps a synthetic line that only exists on the comparison date', () => {
    const sheet = buildBalanceSheet(result({}), result({ unrealizedGains: u(40) }));
    expect(sheet.equity.rows.find((r) => r.name === 'Unrealized gains')).toEqual(
      expect.objectContaining({ amount: u(0), compareAmount: u(40) })
    );
    expect(sheet.compareTotalLiabilitiesAndEquity).toBe(u(40));
  });

  it('leaves comparison fields empty without a second date', () => {
    const sheet = buildBalanceSheet(result({ trees: tree(300) }));
    expect(sheet.assets.rows[0].compareAmount).toBeNull();
    expect(sheet.assets.compareTotal).toBeNull();
  });
});

describe('comparisonDate', () => {
  it('finds the end of the previous month, year and the same day a year back', () => {
    expect(comparisonDate('2026-03-15', 'month')).toBe('2026-02-28');
    expect(comparisonDate('2026-01-10', 'month')).toBe('2025-12-31');
    expect(comparisonDate('2026-10-25', 'year')).toBe('2025-12-31');
    expect(comparisonDate('2026-10-25', 'yoy')).toBe('2025-10-25');
    expect(comparisonDate('2024-02-29', 'yoy')).toBe('2023-02-28');
  });
});
