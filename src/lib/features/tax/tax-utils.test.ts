import { describe, expect, it } from 'vitest';
import type { AccountNode, TaxRole, TaxSummary } from '$lib/tauri';
import {
  hasTaxData,
  inheritedRole,
  roleRows,
  rolesFor,
  toAssignments,
  yearRange
} from './tax-utils';

function node(
  guid: string,
  accountType: string,
  children: AccountNode[] = [],
  code: string | null = null
): AccountNode {
  return {
    guid,
    name: guid,
    accountType,
    code,
    description: null,
    placeholder: false,
    commodity: 'EUR',
    commodityGuid: 'c-eur',
    balance: 0,
    txCount: 0,
    children
  };
}

const trees = [
  node('root', 'ROOT', [
    node('bank', 'BANK'),
    node('income', 'INCOME', [
      node('payroll', 'INCOME'),
      node('interests', 'INCOME', [node('revolut', 'INCOME')])
    ]),
    node('taxes', 'EXPENSE', [node('irpf', 'EXPENSE')])
  ])
];

describe('rolesFor', () => {
  it('splits roles by account type', () => {
    expect(rolesFor('INCOME')).toEqual(['employment_income', 'savings_income', 'exempt']);
    expect(rolesFor('EXPENSE')).toContain('tax_settlement');
    expect(rolesFor('BANK')).toEqual([]);
  });
});

describe('roleRows', () => {
  it('keeps income and expense accounts in tree order with relative depth', () => {
    const rows = roleRows(trees);
    expect(rows.map((r) => [r.guid, r.depth])).toEqual([
      ['income', 0],
      ['payroll', 1],
      ['interests', 1],
      ['revolut', 2],
      ['taxes', 0],
      ['irpf', 1]
    ]);
    expect(rows.find((r) => r.guid === 'income')?.parentGuid).toBeNull();
  });
});

describe('inheritedRole', () => {
  it('walks up to the nearest ancestor with a role', () => {
    const rows = roleRows(trees);
    const roles = new Map<string, TaxRole>([['interests', 'savings_income']]);
    expect(inheritedRole('revolut', rows, roles)).toBe('savings_income');
    expect(inheritedRole('interests', rows, roles)).toBeNull();
    expect(inheritedRole('payroll', rows, roles)).toBeNull();
  });
});

describe('toAssignments', () => {
  it('drops accounts that are no longer listed', () => {
    const rows = roleRows(trees);
    const roles = new Map<string, TaxRole>([
      ['payroll', 'employment_income'],
      ['gone', 'exempt']
    ]);
    expect(toAssignments(roles, rows)).toEqual([
      { accountGuid: 'payroll', role: 'employment_income' }
    ]);
  });
});

describe('hasTaxData', () => {
  const empty = { lines: [], total: 0 };
  const base = {
    employmentIncome: empty,
    employmentWithholding: empty,
    socialContributions: empty,
    savingsIncome: empty,
    savingsWithholding: empty,
    settlement: empty,
    exempt: empty,
    realized: { gains: 0, losses: 0, net: 0, disposals: 0 }
  } as unknown as TaxSummary;

  it('is false until a role account or a sale has activity', () => {
    expect(hasTaxData(base)).toBe(false);
    expect(hasTaxData({ ...base, realized: { ...base.realized, disposals: 1 } })).toBe(true);
  });
});

describe('yearRange', () => {
  it('covers the calendar year', () => {
    expect(yearRange(2025)).toEqual({ from: '2025-01-01', to: '2025-12-31' });
  });
});
