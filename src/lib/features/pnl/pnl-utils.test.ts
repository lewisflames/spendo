import { describe, expect, it } from 'vitest';
import { changePct } from './pnl-utils';

describe('changePct', () => {
  it('returns null when both periods are zero', () => {
    expect(changePct(0, 0)).toBeNull();
  });

  it('reports 100% for a new account with no prior balance', () => {
    expect(changePct(50, 0)).toBe(100);
    expect(changePct(-50, 0)).toBe(100);
  });

  it('computes relative change otherwise', () => {
    expect(changePct(150, 100)).toBeCloseTo(50);
    expect(changePct(50, 100)).toBeCloseTo(-50);
    expect(changePct(-150, -100)).toBeCloseTo(50);
  });
});
