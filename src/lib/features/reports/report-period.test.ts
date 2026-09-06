import { CalendarDate } from '@internationalized/date';
import { describe, expect, it } from 'vitest';
import {
  formatPeriodLabel,
  fromAbsMonth,
  fromAbsQuarter,
  getMonthShortNames,
  getReportYearBounds,
  inferPeriodTab,
  isSameRange,
  rangeFromAbsMonths,
  rangeFromAbsQuarters,
  rangeFromYears,
  toAbsMonth,
  toAbsQuarter
} from './report-period';

function cd(year: number, month: number, day: number): CalendarDate {
  return new CalendarDate(year, month, day);
}

function str(range: { start: CalendarDate; end: CalendarDate }): [string, string] {
  return [range.start.toString(), range.end.toString()];
}

describe('absolute month/quarter math', () => {
  it('round-trips months', () => {
    expect(fromAbsMonth(toAbsMonth(2024, 1))).toEqual({ year: 2024, month: 1 });
    expect(fromAbsMonth(toAbsMonth(2024, 12))).toEqual({ year: 2024, month: 12 });
    expect(toAbsMonth(2024, 2)).toBeGreaterThan(toAbsMonth(2024, 1));
  });

  it('round-trips quarters', () => {
    expect(fromAbsQuarter(toAbsQuarter(2024, 1))).toEqual({ year: 2024, quarter: 1 });
    expect(fromAbsQuarter(toAbsQuarter(2024, 4))).toEqual({ year: 2024, quarter: 4 });
  });

  it('normalizes reversed month bounds', () => {
    expect(str(rangeFromAbsMonths(toAbsMonth(2024, 6), toAbsMonth(2024, 3)))).toEqual([
      '2024-03-01',
      '2024-06-30'
    ]);
  });

  it('normalizes reversed quarter bounds', () => {
    expect(str(rangeFromAbsQuarters(toAbsQuarter(2024, 2), toAbsQuarter(2024, 1)))).toEqual([
      '2024-01-01',
      '2024-06-30'
    ]);
  });

  it('covers whole years in order', () => {
    expect(str(rangeFromYears(2025, 2024))).toEqual(['2024-01-01', '2025-12-31']);
  });
});

describe('getReportYearBounds', () => {
  it('derives bounds from the book window', () => {
    const { minYear, maxYear } = getReportYearBounds('2021-05-01', '2023-08-01');
    expect(minYear).toBe(2021);
    expect(maxYear).toBeGreaterThanOrEqual(new Date().getFullYear());
  });

  it('falls back when the book has no dates', () => {
    const currentYear = new Date().getFullYear();
    expect(getReportYearBounds(null, null)).toEqual({
      minYear: currentYear - 20,
      maxYear: currentYear
    });
  });

  it('swaps inverted bounds instead of returning an empty range', () => {
    const bounds = getReportYearBounds('2025-01-01', '2020-01-01');
    expect(bounds.minYear).toBeLessThanOrEqual(bounds.maxYear);
  });
});

describe('formatPeriodLabel', () => {
  it('returns null without both bounds', () => {
    expect(formatPeriodLabel(null, cd(2024, 1, 1))).toBeNull();
    expect(formatPeriodLabel(cd(2024, 1, 1), undefined)).toBeNull();
  });

  it('labels aligned year ranges compactly', () => {
    expect(formatPeriodLabel(cd(2024, 1, 1), cd(2024, 12, 31))).toBe('2024');
    expect(formatPeriodLabel(cd(2023, 1, 1), cd(2024, 12, 31))).toBe('2023 – 2024');
  });

  it('labels aligned quarter ranges compactly', () => {
    expect(formatPeriodLabel(cd(2024, 1, 1), cd(2024, 3, 31))).toBe('Q1 2024');
    expect(formatPeriodLabel(cd(2024, 1, 1), cd(2024, 6, 30))).toBe('Q1–Q2 2024');
    expect(formatPeriodLabel(cd(2023, 10, 1), cd(2024, 3, 31))).toBe('Q4 2023 – Q1 2024');
  });

  it('labels month ranges with month names', () => {
    const single = formatPeriodLabel(cd(2024, 1, 1), cd(2024, 1, 31));
    expect(single).toContain('2024');
    expect(single).toMatch(/january/i);
    const multi = formatPeriodLabel(cd(2024, 1, 1), cd(2024, 6, 30));
    expect(multi).toContain('2024');
    expect(multi).toContain('–');
  });

  it('falls back to exact dates for unaligned ranges', () => {
    const label = formatPeriodLabel(cd(2024, 1, 5), cd(2024, 3, 12));
    expect(label).toContain('2024');
    expect(label).toContain('–');
  });
});

describe('inferPeriodTab', () => {
  it('picks year and quarter for aligned ranges', () => {
    expect(inferPeriodTab(cd(2024, 1, 1), cd(2024, 12, 31))).toBe('year');
    expect(inferPeriodTab(cd(2024, 1, 1), cd(2024, 3, 31))).toBe('quarter');
  });

  it('defaults to month otherwise', () => {
    expect(inferPeriodTab(cd(2024, 1, 1), cd(2024, 2, 29))).toBe('month');
    expect(inferPeriodTab(null, null)).toBe('month');
  });
});

describe('isSameRange / getMonthShortNames', () => {
  const range = { start: cd(2024, 1, 1), end: cd(2024, 1, 31) };

  it('compares by day, not identity', () => {
    expect(isSameRange(range, { start: cd(2024, 1, 1), end: cd(2024, 1, 31) })).toBe(true);
    expect(isSameRange(range, { start: cd(2024, 1, 1), end: cd(2024, 2, 29) })).toBe(false);
    expect(isSameRange(range, { start: null, end: cd(2024, 1, 31) })).toBe(false);
  });

  it('returns twelve short month names', () => {
    const names = getMonthShortNames('en-US');
    expect(names.length).toBe(12);
    expect(names[0]).toMatch(/jan/i);
  });
});
