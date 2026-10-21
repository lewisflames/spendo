import { describe, expect, it } from 'vitest';
import type { AccountNode, AccountsResult } from '$lib/tauri';
import { buildAccountSections, parentGuids, visibleAccountRows } from './account-tree';

function node(guid: string, accountType: string, children: AccountNode[] = [], balance = 0) {
  return {
    guid,
    name: guid,
    accountType,
    code: null,
    description: null,
    placeholder: false,
    commodity: null,
    commodityGuid: null,
    balance,
    txCount: 0,
    children
  } satisfies AccountNode;
}

const checking = node('Checking', 'BANK', [], 100);
const savings = node('Savings', 'BANK', [], 50);
const bank = node('Bank', 'ASSET', [checking, savings]);
const assetsRoot = { ...node('Assets', 'ASSET', [bank], 150), placeholder: true };
const groceries = node('Groceries', 'EXPENSE', [], 20);

const data = {
  trees: [assetsRoot, groceries],
  byType: {},
  totalAccounts: 5,
  reportCurrency: 'EUR',
  retainedEarnings: 0,
  unrealizedGains: 0,
  balanceCheck: 0,
  missingFx: [],
  missingPrices: []
} satisfies AccountsResult;

describe('buildAccountSections', () => {
  it('skips placeholder category roots and orders sections', () => {
    const sections = buildAccountSections(data);
    expect(sections.map((s) => s.category)).toEqual(['ASSET', 'EXPENSE']);
    expect(sections[0].roots.map((r) => r.guid)).toEqual(['Bank']);
    expect(sections[0].total).toBe(150);
  });
});

describe('visibleAccountRows', () => {
  it('shows children only for expanded parents', () => {
    expect(visibleAccountRows([bank], new Set()).map((r) => r.node.guid)).toEqual(['Bank']);
    const rows = visibleAccountRows([bank], new Set(['Bank']));
    expect(rows.map((r) => [r.node.guid, r.depth])).toEqual([
      ['Bank', 0],
      ['Checking', 1],
      ['Savings', 1]
    ]);
    expect(rows[0].total).toBe(150);
  });

  it('expands the path to search matches and hides the rest', () => {
    const rows = visibleAccountRows([bank], new Set(), 'sav');
    expect(rows.map((r) => r.node.guid)).toEqual(['Bank', 'Savings']);
  });

  it('lists parent guids for expand all', () => {
    expect(parentGuids([assetsRoot])).toEqual(['Assets', 'Bank']);
  });
});
