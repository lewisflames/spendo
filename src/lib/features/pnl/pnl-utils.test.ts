import { describe, expect, it } from 'vitest';
import { signColor } from '$lib/utils/format.svelte';
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

describe('signColor (variance semantics)', () => {
  it('marks non-negative as good when positive is good', () => {
    expect(signColor(10, true)).toBe('text-success');
    expect(signColor(0, true)).toBe('text-success');
    expect(signColor(-1, true)).toBe('text-destructive');
  });

  it('inverts when positive is bad', () => {
    expect(signColor(10, false)).toBe('text-destructive');
    expect(signColor(-1, false)).toBe('text-success');
  });
});
