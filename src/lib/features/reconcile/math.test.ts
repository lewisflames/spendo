import { describe, expect, it } from 'vitest';
import {
  clearedTotal,
  duplicateCandidateGuids,
  isBalanced,
  isReconcilable,
  parseStatementAmount,
  rangeSplitGuids,
  selectionState,
  statementDifference,
  targetBalance
} from './math';
import { toUnits as u } from '$lib/utils/units';

describe('reconcile statement math', () => {
  it('sums cleared amounts and derives the target', () => {
    expect(clearedTotal([u(10), u(-30.5)])).toBe(u(-20.5));
    expect(targetBalance(u(100), u(-30))).toBe(u(70));
  });

  it('adds cent amounts without float drift', () => {
    // 0.1 + 0.2 is 0.30000000000000004 in floats; units add as integers.
    expect(clearedTotal([u(0.1), u(0.2)])).toBe(u(0.3));
    expect(isBalanced(statementDifference(u(0.3), clearedTotal([u(0.1), u(0.2)])))).toBe(true);
  });

  it('computes the statement difference', () => {
    expect(statementDifference(u(70), u(70))).toBe(0);
    expect(statementDifference(u(70), u(50))).toBe(u(20));
  });

  it('balances only on an exact zero', () => {
    expect(isBalanced(u(0))).toBe(true);
    expect(isBalanced(u(0.0001))).toBe(false);
    expect(isBalanced(u(-1))).toBe(false);
  });

  it('gates reconcilable account types', () => {
    for (const type of [
      'ASSET',
      'BANK',
      'CASH',
      'CURRENCY',
      'RECEIVABLE',
      'LIABILITY',
      'CREDIT',
      'PAYABLE'
    ]) {
      expect(isReconcilable(type, false)).toBe(true);
    }
    expect(isReconcilable('BANK', true)).toBe(false);
    expect(isReconcilable('STOCK', false)).toBe(false);
    expect(isReconcilable('EXPENSE', false)).toBe(false);
    expect(isReconcilable('ROOT', false)).toBe(false);
  });

  it('parses statement amounts with locale-aware grouping', () => {
    expect(parseStatementAmount('')).toBeNull();
    expect(parseStatementAmount('   ')).toBeNull();
    expect(parseStatementAmount('70')).toBe(u(70));
    expect(parseStatementAmount(' 70.50 ')).toBe(u(70.5));
    expect(parseStatementAmount('-12,5')).toBe(u(-12.5));
    expect(parseStatementAmount('abc')).toBeNull();
    expect(parseStatementAmount('12.34.56')).toBeNull();
    // Grouped values round-trip instead of disabling Finish.
    expect(parseStatementAmount('1,000.50')).toBe(u(1000.5));
    expect(parseStatementAmount('1,234,567.89')).toBe(u(1234567.89));
    expect(parseStatementAmount('1.000,50', 'de-DE')).toBe(u(1000.5));
    expect(parseStatementAmount('1 234,56', 'fr-FR')).toBe(u(1234.56));
    expect(parseStatementAmount("1'234.56", 'de-CH')).toBe(u(1234.56));
    // A lone three-digit group follows the locale's separator roles.
    expect(parseStatementAmount('1,000')).toBe(u(1000));
    expect(parseStatementAmount('1.000', 'de-DE')).toBe(u(1000));
    // Unknown separator roles stay ambiguous rather than guessing.
    expect(parseStatementAmount('1.000', 'fr-FR')).toBeNull();
    // Malformed grouping is rejected, not silently flattened.
    expect(parseStatementAmount('12,34,56')).toBeNull();
    expect(parseStatementAmount('1234,567')).toBeNull();
    // A short fraction still reads as decimal even with grouping characters.
    expect(parseStatementAmount('12.34', 'de-DE')).toBe(u(12.34));
    // Exact to the unit; a fifth decimal can't be stored, so it's rejected.
    expect(parseStatementAmount('0.1234')).toBe(1234);
    expect(parseStatementAmount('0.12345')).toBeNull();
  });

  it('derives header checkbox tri-state', () => {
    const rows = [{ splitGuid: 'a' }, { splitGuid: 'b' }];
    expect(selectionState(new Set(), rows)).toBe('none');
    expect(selectionState(new Set(), [])).toBe('none');
    expect(selectionState(new Set(['a']), rows)).toBe('some');
    expect(selectionState(new Set(['a', 'b']), rows)).toBe('all');
  });

  it('selects row ranges inclusively in either direction', () => {
    const rows = [{ splitGuid: 'a' }, { splitGuid: 'b' }, { splitGuid: 'c' }];
    expect(rangeSplitGuids(rows, 'a', 'c')).toEqual(['a', 'b', 'c']);
    expect(rangeSplitGuids(rows, 'c', 'a')).toEqual(['a', 'b', 'c']);
    expect(rangeSplitGuids(rows, null, 'b')).toEqual(['b']);
    expect(rangeSplitGuids(rows, 'missing', 'b')).toEqual(['b']);
    expect(rangeSplitGuids(rows, 'a', 'missing')).toEqual([]);
  });

  it('flags candidates sharing a date and amount', () => {
    const rows = [
      { splitGuid: 'a', postDate: '2024-01-01', amount: u(10) },
      { splitGuid: 'b', postDate: '2024-01-01', amount: u(10) },
      { splitGuid: 'c', postDate: '2024-01-01', amount: u(10.01) },
      { splitGuid: 'd', postDate: '2024-01-02', amount: u(10) }
    ];
    expect([...duplicateCandidateGuids(rows)].sort()).toEqual(['a', 'b']);
    expect(duplicateCandidateGuids([]).size).toBe(0);
  });
});
