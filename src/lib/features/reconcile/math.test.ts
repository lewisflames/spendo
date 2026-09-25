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

describe('reconcile statement math', () => {
  it('sums cleared amounts and derives the target', () => {
    expect(clearedTotal([10, -30.5])).toBeCloseTo(-20.5, 10);
    expect(targetBalance(100, -30)).toBeCloseTo(70, 10);
  });

  it('computes the statement difference', () => {
    expect(statementDifference(70, 70)).toBeCloseTo(0, 10);
    expect(statementDifference(70, 50)).toBeCloseTo(20, 10);
  });

  it('balances within half-cent tolerance', () => {
    expect(isBalanced(0)).toBe(true);
    expect(isBalanced(0.004)).toBe(true);
    expect(isBalanced(0.006)).toBe(false);
    expect(isBalanced(-1)).toBe(false);
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
    expect(parseStatementAmount('70')).toBe(70);
    expect(parseStatementAmount(' 70.50 ')).toBeCloseTo(70.5, 10);
    expect(parseStatementAmount('-12,5')).toBeCloseTo(-12.5, 10);
    expect(parseStatementAmount('abc')).toBeNull();
    expect(parseStatementAmount('12.34.56')).toBeNull();
    // Grouped values round-trip instead of disabling Finish.
    expect(parseStatementAmount('1,000.50')).toBeCloseTo(1000.5, 10);
    expect(parseStatementAmount('1,234,567.89')).toBeCloseTo(1234567.89, 10);
    expect(parseStatementAmount('1.000,50', 'de-DE')).toBeCloseTo(1000.5, 10);
    expect(parseStatementAmount('1 234,56', 'fr-FR')).toBeCloseTo(1234.56, 10);
    expect(parseStatementAmount("1'234.56", 'de-CH')).toBeCloseTo(1234.56, 10);
    // A lone three-digit group follows the locale's separator roles.
    expect(parseStatementAmount('1,000')).toBe(1000);
    expect(parseStatementAmount('1.000', 'de-DE')).toBe(1000);
    // Unknown separator roles stay ambiguous rather than guessing.
    expect(parseStatementAmount('1.000', 'fr-FR')).toBeNull();
    // Malformed grouping is rejected, not silently flattened.
    expect(parseStatementAmount('12,34,56')).toBeNull();
    expect(parseStatementAmount('1234,567')).toBeNull();
    // A short fraction still reads as decimal even with grouping characters.
    expect(parseStatementAmount('12.34', 'de-DE')).toBeCloseTo(12.34, 10);
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
      { splitGuid: 'a', postDate: '2024-01-01', amount: 10 },
      { splitGuid: 'b', postDate: '2024-01-01', amount: 10 },
      { splitGuid: 'c', postDate: '2024-01-01', amount: 10.004 },
      { splitGuid: 'd', postDate: '2024-01-02', amount: 10 }
    ];
    expect([...duplicateCandidateGuids(rows)].sort()).toEqual(['a', 'b', 'c']);
    expect(duplicateCandidateGuids([]).size).toBe(0);
  });
});
