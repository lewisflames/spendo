import { describe, expect, it } from 'vitest';
import {
  autoFillIndex,
  draftFromSplit,
  formatDraftAmount,
  isBalanced,
  isInvestmentAccountType,
  makeSplitDraft,
  parseAmount,
  remainderFor,
  splitBalance,
  splitErrors,
  splitToInput,
  splitTotals
} from './split-draft';

describe('parseAmount', () => {
  it('parses finite decimals and trims whitespace', () => {
    expect(parseAmount(' 12.5 ')).toBe(12.5);
    expect(parseAmount('-3')).toBe(-3);
    expect(parseAmount('+0.25')).toBe(0.25);
  });

  it('treats a comma as the decimal separator', () => {
    expect(parseAmount('1,50')).toBe(1.5);
    expect(parseAmount('-2,25')).toBe(-2.25);
  });

  it('accepts thousands separators and currency symbols', () => {
    expect(parseAmount('1,000')).toBe(1000);
    expect(parseAmount('1,000.50')).toBe(1000.5);
    expect(parseAmount('1 000.50')).toBe(1000.5);
    expect(parseAmount("1'000.50")).toBe(1000.5);
    expect(parseAmount('$12.50')).toBe(12.5);
    expect(parseAmount('1.000,50')).toBe(1000.5);
  });

  it('evaluates simple arithmetic', () => {
    expect(parseAmount('10+5')).toBe(15);
    expect(parseAmount('100/4')).toBe(25);
    expect(parseAmount('(10+2)*3')).toBe(36);
    expect(parseAmount('10 - 4 - 1')).toBe(5);
  });

  it('rejects hex, scientific notation and malformed separators', () => {
    expect(parseAmount('0x10')).toBeNull();
    expect(parseAmount('1e3')).toBeNull();
    expect(parseAmount('1.2.3')).toBeNull();
    expect(parseAmount('1,')).toBeNull();
    expect(parseAmount(',5')).toBeNull();
    expect(parseAmount('Infinity')).toBeNull();
    expect(parseAmount('NaN')).toBeNull();
    expect(parseAmount('10/0')).toBeNull();
    expect(parseAmount('(10+')).toBeNull();
  });

  it('returns null for empty or non-numeric input', () => {
    expect(parseAmount('')).toBeNull();
    expect(parseAmount('   ')).toBeNull();
    expect(parseAmount('abc')).toBeNull();
  });
});

describe('makeSplitDraft', () => {
  it('creates an empty draft with the given key', () => {
    expect(makeSplitDraft(7)).toEqual({
      key: 7,
      accountGuid: '',
      amount: '',
      quantity: '',
      showQuantity: false,
      memo: ''
    });
  });
});

describe('draftFromSplit', () => {
  it('hides quantity when it equals the amount', () => {
    const draft = draftFromSplit({ accountGuid: 'a', amount: 10, quantity: 10 }, 1);
    expect(draft.amount).toBe('10');
    expect(draft.showQuantity).toBe(false);
    expect(draft.quantity).toBe('');
  });

  it('shows quantity when it differs from the amount', () => {
    const draft = draftFromSplit({ accountGuid: 'a', amount: 100, quantity: 10, memo: 'buy' }, 2);
    expect(draft.showQuantity).toBe(true);
    expect(draft.quantity).toBe('10');
    expect(draft.memo).toBe('buy');
  });

  it('handles null and missing quantity', () => {
    expect(draftFromSplit({ accountGuid: 'a', amount: 5, quantity: null }, 3).showQuantity).toBe(
      false
    );
    expect(draftFromSplit({ accountGuid: 'a', amount: 5 }, 4).showQuantity).toBe(false);
  });
});

describe('splitBalance / isBalanced', () => {
  it('sums parsed amounts', () => {
    const splits = [
      { ...makeSplitDraft(1), amount: '10' },
      { ...makeSplitDraft(2), amount: '-10' }
    ];
    expect(splitBalance(splits)).toBe(0);
    expect(isBalanced(splitBalance(splits))).toBe(true);
  });

  it('returns null when any amount is missing or invalid', () => {
    const splits = [
      { ...makeSplitDraft(1), amount: '10' },
      { ...makeSplitDraft(2), amount: '' }
    ];
    expect(splitBalance(splits)).toBeNull();
    expect(isBalanced(null)).toBe(false);
  });

  it('treats near-zero sums as balanced', () => {
    const splits = [
      { ...makeSplitDraft(1), amount: '0.1' },
      { ...makeSplitDraft(2), amount: '-0.1' }
    ];
    expect(isBalanced(splitBalance(splits))).toBe(true);
  });
});

describe('splitTotals / remainderFor', () => {
  it('splits debits and credits', () => {
    const splits = [
      { ...makeSplitDraft(1), amount: '100' },
      { ...makeSplitDraft(2), amount: '-40' },
      { ...makeSplitDraft(3), amount: '-60' }
    ];
    expect(splitTotals(splits)).toEqual({ debit: 100, credit: 100 });
  });

  it('returns null totals when any amount is invalid', () => {
    const splits = [
      { ...makeSplitDraft(1), amount: '10' },
      { ...makeSplitDraft(2), amount: '' }
    ];
    expect(splitTotals(splits)).toBeNull();
  });

  it('computes the balancing remainder for a row', () => {
    const splits = [
      { ...makeSplitDraft(1), amount: '100' },
      { ...makeSplitDraft(2), amount: '' }
    ];
    expect(remainderFor(1, splits)).toBe(-100);
    expect(remainderFor(0, [{ ...makeSplitDraft(1), amount: '' }, splits[1]])).toBeNull();
  });

  it('formats computed amounts without float dust', () => {
    expect(formatDraftAmount(0.1 + 0.2 - 0.3)).toBe('0');
    expect(formatDraftAmount(-100)).toBe('-100');
  });

  it('detects investment account types', () => {
    expect(isInvestmentAccountType('STOCK')).toBe(true);
    expect(isInvestmentAccountType('mutual')).toBe(true);
    expect(isInvestmentAccountType('BANK')).toBe(false);
    expect(isInvestmentAccountType(null)).toBe(false);
  });
});

describe('autoFillIndex', () => {
  it('finds the only empty row once the others carry amounts', () => {
    const splits = [
      { ...makeSplitDraft(1), amount: '100' },
      { ...makeSplitDraft(2), amount: '-40' },
      { ...makeSplitDraft(3), amount: '' }
    ];
    expect(autoFillIndex(splits)).toBe(2);
  });

  it('returns -1 when several rows are empty or none carry an amount', () => {
    const twoEmpty = [
      { ...makeSplitDraft(1), amount: '100' },
      { ...makeSplitDraft(2), amount: '' },
      { ...makeSplitDraft(3), amount: '' }
    ];
    expect(autoFillIndex(twoEmpty)).toBe(-1);
    expect(autoFillIndex([makeSplitDraft(1), makeSplitDraft(2)])).toBe(-1);
  });

  it('treats unparseable amounts as empty', () => {
    const splits = [
      { ...makeSplitDraft(1), amount: '10+5' },
      { ...makeSplitDraft(2), amount: 'oops' }
    ];
    expect(autoFillIndex(splits)).toBe(1);
  });
});

describe('splitErrors', () => {
  it('flags missing accounts and amounts per split', () => {
    const splits = [
      { ...makeSplitDraft(1), accountGuid: '', amount: '10' },
      { ...makeSplitDraft(2), accountGuid: 'a', amount: '' },
      { ...makeSplitDraft(3), accountGuid: 'a', amount: '1' }
    ];
    expect(splitErrors(splits)).toEqual([
      'Split 1: choose an account',
      'Split 2: enter an amount',
      ''
    ]);
  });

  it('flags invalid quantity only when shown', () => {
    const shown = {
      ...makeSplitDraft(1),
      accountGuid: 'a',
      amount: '1',
      showQuantity: true,
      quantity: 'x'
    };
    const hidden = { ...shown, showQuantity: false };
    expect(splitErrors([shown])[0]).toBe('Split 1: enter the share count as a number');
    expect(splitErrors([hidden])[0]).toBe('');
  });
});

describe('splitToInput', () => {
  it('maps a draft to the backend input shape', () => {
    const draft = {
      ...makeSplitDraft(1),
      accountGuid: 'a',
      amount: '12.50',
      showQuantity: true,
      quantity: '2',
      memo: ' note '
    };
    expect(splitToInput(draft)).toEqual({
      accountGuid: 'a',
      amount: 12.5,
      quantity: 2,
      memo: 'note'
    });
  });

  it('omits quantity and memo when unset', () => {
    const draft = { ...makeSplitDraft(1), accountGuid: 'a', amount: '5' };
    expect(splitToInput(draft)).toEqual({
      accountGuid: 'a',
      amount: 5,
      quantity: null,
      memo: null
    });
  });
});
