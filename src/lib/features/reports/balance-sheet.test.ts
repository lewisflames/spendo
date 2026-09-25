import { describe, expect, it } from 'vitest';
import type { AccountNode, AccountsResult } from '$lib/tauri';
import { balanceSheetTrees, buildBalanceSheet, subtreeTotal } from './balance-sheet';

function node(partial: Partial<AccountNode> & { guid: string; name: string }): AccountNode {
  return {
    accountType: 'BANK',
    code: null,
    description: null,
    placeholder: false,
    commodity: 'USD',
    commodityGuid: null,
    balance: 0,
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
    retainedEarnings: 0,
    unrealizedGains: 0,
    balanceCheck: 0,
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
      balance: 150,
      children: [
        node({ guid: 'checking', name: 'Checking', balance: 100 }),
        node({ guid: 'savings', name: 'Savings', balance: 50 })
      ]
    });
    expect(subtreeTotal(assets)).toBe(150);
  });

  it('adds the subtree for non-placeholder parents', () => {
    const parent = node({
      guid: 'parent',
      name: 'Parent',
      balance: 40,
      children: [
        node({ guid: 'a', name: 'A', balance: 10 }),
        node({
          guid: 'b',
          name: 'B',
          balance: 5,
          children: [node({ guid: 'c', name: 'C', balance: 2 })]
        })
      ]
    });
    expect(subtreeTotal(parent)).toBe(57);
  });
});

describe('balanceSheetTrees', () => {
  it('keeps only balance-sheet roots, in statement order', () => {
    const trees = [
      node({ guid: 'income', name: 'Income', accountType: 'INCOME', placeholder: true }),
      node({ guid: 'assets', name: 'Assets', accountType: 'ASSET', placeholder: true }),
      node({ guid: 'equity', name: 'Equity', accountType: 'EQUITY', placeholder: true }),
      node({
        guid: 'liabilities',
        name: 'Liabilities',
        accountType: 'LIABILITY',
        placeholder: true
      }),
      node({ guid: 'expenses', name: 'Expenses', accountType: 'EXPENSE', placeholder: true })
    ];
    expect(balanceSheetTrees(trees).map((t) => t.guid)).toEqual([
      'assets',
      'liabilities',
      'equity'
    ]);
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
        balance: 1050,
        children: [
          node({ guid: 'checking', name: 'Checking', balance: 650 }),
          node({ guid: 'brokerage', name: 'Brokerage', accountType: 'STOCK', balance: 400 })
        ]
      }),
      node({
        guid: 'income',
        name: 'Income',
        accountType: 'INCOME',
        placeholder: true,
        balance: 500
      }),
      node({
        guid: 'liabilities',
        name: 'Liabilities',
        accountType: 'LIABILITY',
        placeholder: true,
        balance: 300,
        children: [node({ guid: 'card', name: 'Card', accountType: 'CREDIT', balance: 300 })]
      }),
      node({
        guid: 'equity',
        name: 'Equity',
        accountType: 'EQUITY',
        placeholder: true,
        balance: 200
      }),
      node({
        guid: 'expenses',
        name: 'Expenses',
        accountType: 'EXPENSE',
        placeholder: true,
        balance: 50
      })
    ],
    byType: {
      ASSET: { count: 3, totalBalance: 1050 },
      LIABILITY: { count: 2, totalBalance: 300 },
      EQUITY: { count: 1, totalBalance: 200 },
      INCOME: { count: 1, totalBalance: 500 },
      EXPENSE: { count: 1, totalBalance: 50 }
    },
    retainedEarnings: 450,
    unrealizedGains: 100
  });

  it('splits rows into statement sections with depth-first indents', () => {
    const sheet = buildBalanceSheet(data);
    // The top-level placeholder root is the section group: the header names
    // it, so it isn't repeated as a row.
    expect(sheet.assets.rows.map((r) => [r.accountGuid, r.depth, r.amount])).toEqual([
      ['checking', 1, 650],
      ['brokerage', 1, 400]
    ]);
    expect(sheet.liabilities.rows.map((r) => r.accountGuid)).toEqual(['card']);
    expect(sheet.assets.label).toBe('Assets');
    expect(sheet.liabilities.total).toBe(300);
  });

  it('keeps non-placeholder top-level roots and nested placeholders', () => {
    const trees = [
      node({
        guid: 'investments',
        name: 'Investments',
        accountType: 'ASSET',
        balance: 0,
        children: [
          node({
            guid: 'brokerage',
            name: 'Brokerage',
            accountType: 'ASSET',
            placeholder: true,
            balance: 400,
            children: [node({ guid: 'vti', name: 'VTI', accountType: 'STOCK', balance: 400 })]
          })
        ]
      }),
      node({ guid: 'loose', name: 'Loose cash', accountType: 'CASH', balance: 25 })
    ];
    const sheet = buildBalanceSheet(
      result({ trees, byType: { ASSET: { count: 3, totalBalance: 425 } } })
    );
    expect(sheet.assets.rows.map((r) => [r.accountGuid, r.depth, r.amount])).toEqual([
      ['investments', 0, 400],
      ['brokerage', 1, 400],
      ['vti', 2, 400],
      ['loose', 0, 25]
    ]);
  });

  it('appends retained earnings and unrealized gains to equity', () => {
    const sheet = buildBalanceSheet(data);
    expect(sheet.equity.rows.map((r) => [r.accountGuid, r.name, r.amount])).toEqual([
      ['', 'Retained earnings', 450],
      ['', 'Unrealized gains', 100]
    ]);
    expect(sheet.equity.total).toBe(750);
    // Equity total includes both synthetic lines, so the sheet balances.
    expect(sheet.totalLiabilitiesAndEquity).toBe(1050);
    expect(sheet.assets.total).toBe(sheet.totalLiabilitiesAndEquity);
  });

  it('hides a zero unrealized gains line', () => {
    const sheet = buildBalanceSheet({ ...data, unrealizedGains: 0 });
    expect(sheet.equity.rows.some((r) => r.name === 'Unrealized gains')).toBe(false);
    expect(sheet.equity.total).toBe(650);
  });

  it('returns empty sections for an empty book', () => {
    const sheet = buildBalanceSheet(result({}));
    expect(sheet.assets.rows).toEqual([]);
    expect(sheet.liabilities.rows).toEqual([]);
    expect(sheet.equity.rows.map((r) => r.name)).toEqual(['Retained earnings']);
    expect(sheet.totalLiabilitiesAndEquity).toBe(0);
  });
});
