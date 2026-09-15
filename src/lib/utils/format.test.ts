import { describe, expect, it } from 'vitest';
import {
  clampZero,
  EMPTY_DASH,
  fmtPct,
  formatDateRange,
  formatRelativeTime,
  parseLocalDate,
  plural,
  signColor,
  yearOf
} from './format.svelte';

describe('signColor', () => {
  it('treats zero as non-negative', () => {
    expect(signColor(0)).toBe('text-success');
    expect(signColor(0, false)).toBe('text-destructive');
  });

  it('flips with positiveGood', () => {
    expect(signColor(5)).toBe('text-success');
    expect(signColor(-5)).toBe('text-destructive');
    expect(signColor(5, false)).toBe('text-destructive');
    expect(signColor(-5, false)).toBe('text-success');
  });
});

describe('fmtPct', () => {
  it('formats with one decimal and explicit positive sign', () => {
    expect(fmtPct(12.345)).toBe('+12.3%');
    expect(fmtPct(-4)).toBe('-4.0%');
    expect(fmtPct(0)).toBe('+0.0%');
  });

  it('renders a dash for nullish input', () => {
    expect(fmtPct(null)).toBe(EMPTY_DASH);
    expect(fmtPct(undefined)).toBe(EMPTY_DASH);
  });
});

describe('clampZero', () => {
  it('snaps float dust to zero', () => {
    expect(clampZero(-0.004)).toBe(0);
    expect(clampZero(0.0049)).toBe(0);
  });

  it('keeps real values', () => {
    expect(clampZero(0.005)).toBe(0.005);
    expect(clampZero(-3.5)).toBe(-3.5);
  });
});

describe('parseLocalDate', () => {
  it('parses date-only strings as local midnight, not UTC', () => {
    const d = parseLocalDate('2024-01-15');
    expect(d.getFullYear()).toBe(2024);
    expect(d.getMonth()).toBe(0);
    expect(d.getDate()).toBe(15);
    expect(d.getHours()).toBe(0);
  });

  it('accepts YYYY-MM as the first of the month', () => {
    const d = parseLocalDate('2024-02');
    expect(d.getFullYear()).toBe(2024);
    expect(d.getMonth()).toBe(1);
    expect(d.getDate()).toBe(1);
  });

  it('passes Date instances through the constructor', () => {
    const src = new Date(2024, 5, 10, 12);
    expect(parseLocalDate(src).getTime()).toBe(src.getTime());
  });
});

describe('yearOf', () => {
  it('reads the year without UTC-shift risk', () => {
    expect(yearOf('2024-01-01')).toBe(2024);
    expect(yearOf('2023-12-31')).toBe(2023);
  });
});

describe('formatDateRange', () => {
  it('labels a bounded range with both years', () => {
    const label = formatDateRange('2024-01-05', '2024-03-12');
    expect(label).toContain('2024');
    expect(label).toContain('–');
  });

  it('keeps layout stable with dash placeholders', () => {
    expect(formatDateRange(null, null)).toBe(`${EMPTY_DASH} – ${EMPTY_DASH}`);
    expect(formatDateRange('2024-01-05', null)).toContain(EMPTY_DASH);
  });
});

describe('plural', () => {
  it('singularizes exactly one', () => {
    expect(plural(1, 'account')).toBe('1 account');
    expect(plural(0, 'account')).toBe('0 accounts');
    expect(plural(3, 'account')).toBe('3 accounts');
  });
});

describe('formatRelativeTime', () => {
  const now = Date.UTC(2026, 0, 15, 12, 0, 0);

  it('collapses sub-minute differences to "just now"', () => {
    expect(formatRelativeTime(now / 1000 - 30, now)).toBe('just now');
    expect(formatRelativeTime(now / 1000, now)).toBe('just now');
  });

  it('picks the coarsest matching unit', () => {
    expect(formatRelativeTime(now / 1000 - 120, now)).toBe('2 minutes ago');
    expect(formatRelativeTime(now / 1000 - 3 * 60 * 60, now)).toBe('3 hours ago');
    expect(formatRelativeTime(now / 1000 - 2 * 24 * 60 * 60, now)).toBe('2 days ago');
  });

  it('handles future timestamps', () => {
    expect(formatRelativeTime(now / 1000 + 2 * 60 * 60, now)).toBe('in 2 hours');
  });
});
