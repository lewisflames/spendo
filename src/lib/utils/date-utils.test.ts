import { CalendarDate } from '@internationalized/date';
import { describe, expect, it } from 'vitest';
import {
  calendarDateToStr,
  computePrevRange,
  daysInMonth,
  getMonthRange,
  getMonthRangeSpan,
  getQuarterRange,
  getYearRange,
  last12MonthsRange,
  monthName,
  PNL_PRESETS,
  todayStr
} from './date-utils.svelte';

describe('todayStr', () => {
  it('emits a local YYYY-MM-DD date', () => {
    const now = new Date();
    const expected = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
    expect(todayStr()).toBe(expected);
  });
});

describe('daysInMonth', () => {
  it('is leap-year aware', () => {
    expect(daysInMonth(2024, 2)).toBe(29);
    expect(daysInMonth(2023, 2)).toBe(28);
    expect(daysInMonth(2024, 4)).toBe(30);
    expect(daysInMonth(2024, 1)).toBe(31);
  });
});

describe('calendar ranges', () => {
  it('covers the full year', () => {
    const { start, end } = getYearRange(2024);
    expect(calendarDateToStr(start)).toBe('2024-01-01');
    expect(calendarDateToStr(end)).toBe('2024-12-31');
  });

  it('covers the full quarter', () => {
    const { start, end } = getQuarterRange(2024, 2);
    expect(calendarDateToStr(start)).toBe('2024-04-01');
    expect(calendarDateToStr(end)).toBe('2024-06-30');
  });

  it('clamps out-of-range quarters and months', () => {
    expect(calendarDateToStr(getQuarterRange(2024, 0).start)).toBe('2024-01-01');
    expect(calendarDateToStr(getQuarterRange(2024, 9).end)).toBe('2024-12-31');
    expect(calendarDateToStr(getMonthRange(2024, 0).start)).toBe('2024-01-01');
    expect(calendarDateToStr(getMonthRange(2024, 13).end)).toBe('2024-12-31');
  });

  it('covers the full leap February', () => {
    const { start, end } = getMonthRange(2024, 2);
    expect(calendarDateToStr(start)).toBe('2024-02-01');
    expect(calendarDateToStr(end)).toBe('2024-02-29');
  });

  it('spans whole months within a year', () => {
    const { start, end } = getMonthRangeSpan(2024, 3, 2024, 6);
    expect(calendarDateToStr(start)).toBe('2024-03-01');
    expect(calendarDateToStr(end)).toBe('2024-06-30');
  });

  it('spans a single month when both ends match', () => {
    const { start, end } = getMonthRangeSpan(2024, 5, 2024, 5);
    expect(calendarDateToStr(start)).toBe('2024-05-01');
    expect(calendarDateToStr(end)).toBe('2024-05-31');
  });

  it('normalizes reversed month order', () => {
    const { start, end } = getMonthRangeSpan(2024, 9, 2024, 4);
    expect(calendarDateToStr(start)).toBe('2024-04-01');
    expect(calendarDateToStr(end)).toBe('2024-09-30');
  });

  it('spans across years in either direction', () => {
    const forward = getMonthRangeSpan(2024, 11, 2025, 2);
    expect(calendarDateToStr(forward.start)).toBe('2024-11-01');
    expect(calendarDateToStr(forward.end)).toBe('2025-02-28');
    const reversed = getMonthRangeSpan(2025, 2, 2024, 11);
    expect(calendarDateToStr(reversed.start)).toBe('2024-11-01');
    expect(calendarDateToStr(reversed.end)).toBe('2025-02-28');
  });

  it('ends leap February correctly in a span', () => {
    const { start, end } = getMonthRangeSpan(2024, 1, 2024, 2);
    expect(calendarDateToStr(start)).toBe('2024-01-01');
    expect(calendarDateToStr(end)).toBe('2024-02-29');
  });
});

describe('monthName', () => {
  it('returns the long month name for the locale', () => {
    expect(monthName(1, 'en-US')).toBe('January');
    expect(monthName(12, 'en-US')).toBe('December');
    expect(monthName(3, 'de-DE')).toBe('März');
  });
});

describe('computePrevRange', () => {
  it('returns the same-length window immediately before', () => {
    expect(computePrevRange('2024-01-01', '2024-01-31')).toEqual({
      prevFrom: '2023-12-01',
      prevTo: '2023-12-31'
    });
  });

  it('handles single-day ranges', () => {
    expect(computePrevRange('2024-03-15', '2024-03-15')).toEqual({
      prevFrom: '2024-03-14',
      prevTo: '2024-03-14'
    });
  });

  it('maps whole years to the whole previous year', () => {
    expect(computePrevRange('2026-01-01', '2026-12-31')).toEqual({
      prevFrom: '2025-01-01',
      prevTo: '2025-12-31'
    });
  });

  it('is leap-safe for whole years across leap boundaries', () => {
    expect(computePrevRange('2024-01-01', '2024-12-31')).toEqual({
      prevFrom: '2023-01-01',
      prevTo: '2023-12-31'
    });
    expect(computePrevRange('2025-01-01', '2025-12-31')).toEqual({
      prevFrom: '2024-01-01',
      prevTo: '2024-12-31'
    });
  });
});

describe('PNL_PRESETS', () => {
  it('offers This year as a full calendar year', () => {
    const preset = PNL_PRESETS.find((p) => p.label === 'This year');
    expect(preset).toBeDefined();
    const year = new Date().getFullYear();
    const { start, end } = preset!.getRange();
    expect(calendarDateToStr(start)).toBe(`${year}-01-01`);
    expect(calendarDateToStr(end)).toBe(`${year}-12-31`);
  });

  it('no longer offers Year to date', () => {
    expect(PNL_PRESETS.some((p) => p.label === 'Year to date')).toBe(false);
  });
});

describe('last12MonthsRange', () => {
  it('spans twelve complete calendar months ending last month', () => {
    const { start, end } = last12MonthsRange();
    expect(start.day).toBe(1);
    expect(end.day).toBe(daysInMonth(end.year, end.month));
    const monthDistance = (end.year - start.year) * 12 + (end.month - start.month);
    expect(monthDistance).toBe(11);
  });

  it('round-trips through calendarDateToStr', () => {
    expect(calendarDateToStr(new CalendarDate(2024, 2, 29))).toBe('2024-02-29');
  });
});
