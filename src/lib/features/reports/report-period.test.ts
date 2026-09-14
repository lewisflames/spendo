import { CalendarDate } from '@internationalized/date';
import { describe, expect, it } from 'vitest';
import {
  formatPeriodLabel,
  getMonthShortNames,
  getReportYearBounds,
  isAtBookStart,
  isAtPresent,
  isSameRange,
  isYearToDateLike,
  shiftPeriodRange
} from './report-period';

function cd(year: number, month: number, day: number): CalendarDate {
  return new CalendarDate(year, month, day);
}

function str(range: { start: CalendarDate; end: CalendarDate }): [string, string] {
  return [range.start.toString(), range.end.toString()];
}

describe('getReportYearBounds', () => {
  it('derives bounds from the book window and never before the current year', () => {
    expect(getReportYearBounds('2021-05-01', '2099-08-01')).toEqual({
      minYear: 2021,
      maxYear: 2099
    });
    const currentYear = new Date().getFullYear();
    expect(getReportYearBounds('2021-05-01', '2023-08-01').maxYear).toBe(
      Math.max(2023, currentYear)
    );
  });

  it('falls back when the book has no dates', () => {
    const currentYear = new Date().getFullYear();
    expect(getReportYearBounds(null, null)).toEqual({
      minYear: currentYear - 20,
      maxYear: currentYear
    });
  });

  it('clamps an inverted window to a valid range', () => {
    const currentYear = new Date().getFullYear();
    expect(getReportYearBounds('9999-01-01', '1900-01-01')).toEqual({
      minYear: currentYear,
      maxYear: currentYear
    });
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

describe('isYearToDateLike', () => {
  it('matches Jan-1-starting partial-year windows', () => {
    expect(isYearToDateLike(cd(2025, 1, 1), cd(2025, 9, 6))).toBe(true);
    expect(isYearToDateLike(cd(2025, 1, 1), cd(2025, 1, 15))).toBe(true);
  });

  it('excludes month-aligned ranges starting Jan 1', () => {
    expect(isYearToDateLike(cd(2025, 1, 1), cd(2025, 1, 31))).toBe(false);
    expect(isYearToDateLike(cd(2025, 1, 1), cd(2025, 3, 31))).toBe(false);
    expect(isYearToDateLike(cd(2025, 1, 1), cd(2025, 12, 31))).toBe(false);
  });

  it('excludes windows not starting Jan 1 or spanning years', () => {
    expect(isYearToDateLike(cd(2025, 2, 1), cd(2025, 9, 6))).toBe(false);
    expect(isYearToDateLike(cd(2024, 1, 1), cd(2025, 9, 6))).toBe(false);
  });
});

describe('shiftPeriodRange', () => {
  it('steps single months with year rollover', () => {
    expect(str(shiftPeriodRange(cd(2024, 3, 1), cd(2024, 3, 31), -1))).toEqual([
      '2024-02-01',
      '2024-02-29'
    ]);
    expect(str(shiftPeriodRange(cd(2024, 12, 1), cd(2024, 12, 31), 1))).toEqual([
      '2025-01-01',
      '2025-01-31'
    ]);
  });

  it('steps quarters by three months', () => {
    expect(str(shiftPeriodRange(cd(2024, 1, 1), cd(2024, 3, 31), 1))).toEqual([
      '2024-04-01',
      '2024-06-30'
    ]);
  });

  it('steps twelve-month windows by twelve months', () => {
    expect(str(shiftPeriodRange(cd(2024, 3, 1), cd(2025, 2, 28), -1))).toEqual([
      '2023-03-01',
      '2024-02-29'
    ]);
  });

  it('shifts other partial ranges by month-span preserving days', () => {
    expect(str(shiftPeriodRange(cd(2024, 3, 5), cd(2024, 5, 20), -1))).toEqual([
      '2023-12-05',
      '2024-02-20'
    ]);
  });

  it('steps YTD-like windows year-over-year', () => {
    expect(str(shiftPeriodRange(cd(2025, 1, 1), cd(2025, 9, 6), -1))).toEqual([
      '2024-01-01',
      '2024-09-06'
    ]);
    expect(str(shiftPeriodRange(cd(2024, 1, 1), cd(2024, 9, 6), 1))).toEqual([
      '2025-01-01',
      '2025-09-06'
    ]);
  });

  it('clamps days when the target month is shorter', () => {
    expect(str(shiftPeriodRange(cd(2024, 1, 30), cd(2024, 1, 31), 1))).toEqual([
      '2024-02-29',
      '2024-02-29'
    ]);
    expect(str(shiftPeriodRange(cd(2023, 3, 15), cd(2023, 3, 31), 1))).toEqual([
      '2023-04-15',
      '2023-04-30'
    ]);
  });
});

describe('isAtBookStart / isAtPresent', () => {
  it('clamps prev at the earliest book month', () => {
    expect(isAtBookStart(cd(2024, 3, 1), '2024-03-15')).toBe(true);
    expect(isAtBookStart(cd(2024, 4, 1), '2024-03-15')).toBe(false);
    expect(isAtBookStart(cd(2024, 2, 1), '2024-03-15')).toBe(true);
    expect(isAtBookStart(cd(2024, 4, 1), null)).toBe(false);
  });

  it('clamps next once the period reaches the current month', () => {
    const now = new Date();
    const y = now.getFullYear();
    const m = now.getMonth() + 1;
    const monthEnd = new Date(y, m, 0).getDate();
    expect(isAtPresent(cd(y, m, 1), cd(y, m, monthEnd))).toBe(true);
    expect(isAtPresent(cd(2020, 3, 1), cd(2020, 3, 31))).toBe(false);
    expect(isAtPresent(cd(y, m, 1), cd(y + 1, m, monthEnd))).toBe(true);
  });

  it('clamps next on the current year-to-date window', () => {
    const now = new Date();
    const y = now.getFullYear();
    const m = now.getMonth() + 1;
    const today = now.getDate();
    // Current YTD blocks forward; last year's same window allows it.
    expect(isAtPresent(cd(y, 1, 1), cd(y, m, today))).toBe(true);
    expect(isAtPresent(cd(y - 1, 1, 1), cd(y - 1, m, today))).toBe(false);
  });
});
