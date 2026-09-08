import { describe, expect, it } from 'vitest';
import { guessMapping } from './guess';

describe('guessMapping', () => {
  it('guesses date/desc/amount from bank headers', () => {
    const g = guessMapping(['Date', 'Description', 'Amount'], 3);
    expect(g).toMatchObject({ dateCol: 0, descCol: 1, amountCol: 2, usePair: false });
  });

  it('prefers the debit+credit pair when both exist', () => {
    const g = guessMapping(['Date', 'Narrative', 'Debit', 'Credit'], 4);
    expect(g.usePair).toBe(true);
    expect(g.debitCol).toBe(2);
    expect(g.creditCol).toBe(3);
  });

  it('falls back to positional defaults without headers', () => {
    const g = guessMapping(['Column 1', 'Column 2', 'Column 3'], 3);
    expect(g.dateCol).toBe(0);
    expect(g.amountCol).toBe(2);
  });
});
