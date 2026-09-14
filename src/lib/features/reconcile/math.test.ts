import { describe, expect, it } from 'vitest';
import {
  clearedTotal,
  isBalanced,
  isReconcilable,
  parseStatementAmount,
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

  it('parses statement amounts strictly', () => {
    expect(parseStatementAmount('')).toBeNull();
    expect(parseStatementAmount('   ')).toBeNull();
    expect(parseStatementAmount('70')).toBe(70);
    expect(parseStatementAmount(' 70.50 ')).toBeCloseTo(70.5, 10);
    expect(parseStatementAmount('-12,5')).toBeCloseTo(-12.5, 10);
    expect(parseStatementAmount('1,000.50')).toBeNull();
    expect(parseStatementAmount('1,000')).toBeNull();
    expect(parseStatementAmount('12,345')).toBeNull();
    expect(parseStatementAmount('abc')).toBeNull();
    expect(parseStatementAmount('12.34.56')).toBeNull();
  });

  it('derives header checkbox tri-state', () => {
    const rows = [{ splitGuid: 'a' }, { splitGuid: 'b' }];
    expect(selectionState(new Set(), rows)).toBe('none');
    expect(selectionState(new Set(), [])).toBe('none');
    expect(selectionState(new Set(['a']), rows)).toBe('some');
    expect(selectionState(new Set(['a', 'b']), rows)).toBe('all');
  });
});
