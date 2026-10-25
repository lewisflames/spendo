import { describe, expect, it } from 'vitest';
import { MAX_PAYEE_LEN, normalizePayeeName, payeeNameError, payeeOptions } from './payee-utils';

describe('normalizePayeeName', () => {
  it('trims and collapses whitespace', () => {
    expect(normalizePayeeName('  Corte   Inglés ')).toBe('Corte Inglés');
    expect(normalizePayeeName('   ')).toBe('');
  });
});

describe('payeeNameError', () => {
  it('allows blank (no payee) and caps the length', () => {
    expect(payeeNameError('')).toBeNull();
    expect(payeeNameError('x'.repeat(MAX_PAYEE_LEN))).toBeNull();
    expect(payeeNameError('x'.repeat(MAX_PAYEE_LEN + 1))).toMatch(/under 100/);
  });
});

describe('payeeOptions', () => {
  it('hints how often each payee is used', () => {
    expect(
      payeeOptions([{ guid: 'p1', name: 'Renfe', transactionCount: 1, lastUsed: '2026-10-19' }])
    ).toEqual([{ description: 'Renfe', hint: '1 transaction' }]);
  });
});
