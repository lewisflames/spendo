import type { CalendarDate } from '@internationalized/date';
import {
  daysInMonth,
  getMonthRange,
  getQuarterRange,
  getYearRange
} from '$lib/utils/date-utils.svelte';
import { formatDate, yearOf } from '$lib/utils/format.svelte';

export type PeriodTab = 'year' | 'quarter' | 'month';

/** Absolute month index for anchor math: Jan 2024 < Feb 2024 < … */
export function toAbsMonth(year: number, month: number): number {
  return year * 12 + (month - 1);
}

export function fromAbsMonth(abs: number): { year: number; month: number } {
  const year = Math.floor(abs / 12);
  return { year, month: abs - year * 12 + 1 };
}

/** Absolute quarter index for anchor math. Quarter q is 1-4. */
export function toAbsQuarter(year: number, q: number): number {
  return year * 4 + (q - 1);
}

export function fromAbsQuarter(abs: number): { year: number; quarter: number } {
  const year = Math.floor(abs / 4);
  return { year, quarter: abs - year * 4 + 1 };
}

export function rangeFromAbsMonths(
  a: number,
  b: number
): { start: CalendarDate; end: CalendarDate } {
  const lo = Math.min(a, b);
  const hi = Math.max(a, b);
  const startParts = fromAbsMonth(lo);
  const endParts = fromAbsMonth(hi);
  return {
    start: getMonthRange(startParts.year, startParts.month).start,
    end: getMonthRange(endParts.year, endParts.month).end
  };
}

export function rangeFromAbsQuarters(
  a: number,
  b: number
): { start: CalendarDate; end: CalendarDate } {
  const lo = Math.min(a, b);
  const hi = Math.max(a, b);
  const startParts = fromAbsQuarter(lo);
  const endParts = fromAbsQuarter(hi);
  return {
    start: getQuarterRange(startParts.year, startParts.quarter).start,
    end: getQuarterRange(endParts.year, endParts.quarter).end
  };
}

export function rangeFromYears(a: number, b: number): { start: CalendarDate; end: CalendarDate } {
  const lo = Math.min(a, b);
  const hi = Math.max(a, b);
  return {
    start: getYearRange(lo).start,
    end: getYearRange(hi).end
  };
}

/** Year bounds for the picker, from the book's transaction window with a fallback. */
export function getReportYearBounds(
  earliestDate: string | null,
  latestDate: string | null
): { minYear: number; maxYear: number } {
  const currentYear = new Date().getFullYear();
  const minYear = earliestDate ? yearOf(earliestDate) : currentYear - 20;
  const latestYear = latestDate ? yearOf(latestDate) : currentYear;
  const maxYear = Math.max(latestYear, currentYear);
  if (minYear > maxYear) return { minYear: maxYear, maxYear };
  return { minYear, maxYear };
}

function isYearAligned(start: CalendarDate, end: CalendarDate): boolean {
  return start.month === 1 && start.day === 1 && end.month === 12 && end.day === 31;
}

function isQuarterAligned(start: CalendarDate, end: CalendarDate): boolean {
  const startQ = start.month === 1 || start.month === 4 || start.month === 7 || start.month === 10;
  const endQ = end.month === 3 || end.month === 6 || end.month === 9 || end.month === 12;
  if (!startQ || !endQ || start.day !== 1 || end.day !== daysInMonth(end.year, end.month)) {
    return false;
  }
  // Whole quarters between two aligned bounds are always contiguous.
  return true;
}

function isMonthAligned(start: CalendarDate, end: CalendarDate): boolean {
  return start.day === 1 && end.day === daysInMonth(end.year, end.month);
}

function formatMonthYear(cd: CalendarDate): string {
  return formatDate(cd.toString(), { month: 'short', year: 'numeric' });
}

/**
 * Smart trigger label: compact accounting labels for aligned ranges
 * (`2024`, `Q1–Q2 2024`, `Jan 2024 – Jun 2025`), exact dates otherwise.
 * Returns null when either bound is missing so callers show the placeholder.
 */
export function formatPeriodLabel(
  start: CalendarDate | null | undefined,
  end: CalendarDate | null | undefined
): string | null {
  if (!start || !end) return null;
  if (isYearAligned(start, end)) {
    return start.year === end.year ? String(start.year) : `${start.year} – ${end.year}`;
  }
  if (isQuarterAligned(start, end)) {
    const startQ = Math.floor((start.month - 1) / 3) + 1;
    const endQ = Math.floor((end.month - 1) / 3) + 1;
    if (start.year === end.year) {
      return startQ === endQ ? `Q${startQ} ${start.year}` : `Q${startQ}–Q${endQ} ${start.year}`;
    }
    return `Q${startQ} ${start.year} – Q${endQ} ${end.year}`;
  }
  if (isMonthAligned(start, end)) {
    if (start.year === end.year && start.month === end.month) {
      return formatDate(start.toString(), { month: 'long', year: 'numeric' });
    }
    return `${formatMonthYear(start)} – ${formatMonthYear(end)}`;
  }
  return `${formatDate(start.toString())} – ${formatDate(end.toString())}`;
}

/** Infer which tab best explains an existing range when the popover opens. */
export function inferPeriodTab(
  start: CalendarDate | null | undefined,
  end: CalendarDate | null | undefined
): PeriodTab {
  if (start && end) {
    if (isYearAligned(start, end)) return 'year';
    if (isQuarterAligned(start, end)) return 'quarter';
  }
  return 'month';
}

/** Short month names for the month grid, respecting the app locale. */
export function getMonthShortNames(locale: string): string[] {
  const fmt = new Intl.DateTimeFormat(locale, { month: 'short' });
  return Array.from({ length: 12 }, (_, i) => fmt.format(new Date(2000, i, 1)));
}

/** True when two ranges cover exactly the same days (for active-preset highlight). */
export function isSameRange(
  a: { start: CalendarDate; end: CalendarDate },
  b: { start: CalendarDate | null | undefined; end: CalendarDate | null | undefined }
): boolean {
  if (!b.start || !b.end) return false;
  return a.start.compare(b.start) === 0 && a.end.compare(b.end) === 0;
}
