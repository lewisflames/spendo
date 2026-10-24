import { describe, expect, it } from 'vitest';
import { changePct, savingsRate } from './pnl-utils';

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

describe('savingsRate', () => {
  it('is null without positive income', () => {
    expect(savingsRate(0, -50)).toBeNull();
    expect(savingsRate(-10, -50)).toBeNull();
  });

  it('is net over income, negative when overspending', () => {
    expect(savingsRate(1000, 250)).toBeCloseTo(25);
    expect(savingsRate(1000, -200)).toBeCloseTo(-20);
  });
});
