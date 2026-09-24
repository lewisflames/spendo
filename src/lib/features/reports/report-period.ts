import { CalendarDate } from '@internationalized/date';
import { daysInMonth } from '$lib/utils/date-utils.svelte';
import { formatDate, yearOf } from '$lib/utils/format.svelte';

/** Year bounds for the picker, from the book’s transaction window with a fallback. */
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

/**
 * True for Jan-1-starting partial-year windows (the "Year to date" shape):
 * month-aligned ranges starting Jan 1 (single January, Q1, full year) are
 * excluded so they keep their calendar-unit step. Everything matching steps
 * year-over-year (±12 months) instead of by its own month-span.
 */
export function isYearToDateLike(start: CalendarDate, end: CalendarDate): boolean {
  return (
    start.month === 1 && start.day === 1 && start.year === end.year && !isMonthAligned(start, end)
  );
}

function absMonthIndex(year: number, month: number): number {
  return year * 12 + (month - 1);
}

function yearMonthOfAbsMonth(abs: number): { year: number; month: number } {
  const year = Math.floor(abs / 12);
  return { year, month: abs - year * 12 + 1 };
}

/** Shift a date by whole months, clamping the day (Feb 29 → Feb 28). */
function shiftMonthsPreservingDay(cd: CalendarDate, months: number): CalendarDate {
  const { year, month } = yearMonthOfAbsMonth(absMonthIndex(cd.year, cd.month) + months);
  return new CalendarDate(year, month, Math.min(cd.day, daysInMonth(year, month)));
}

/**
 * Shift a period for the prev/next arrows. Aligned ranges move by their own
 * calendar span (month → 1, quarter → 3, 12 months → 12) keeping month
 * boundaries; YTD-like windows move year-over-year (±12); other partial
 * ranges move by their month-span preserving days. Manual year/month math
 * avoids `CalendarDate.add` day-overflow (Jan 31 + 1 mo → March).
 */
export function shiftPeriodRange(
  start: CalendarDate,
  end: CalendarDate,
  direction: 1 | -1
): { start: CalendarDate; end: CalendarDate } {
  const span = isYearToDateLike(start, end)
    ? 12
    : (end.year - start.year) * 12 + (end.month - start.month) + 1;
  const delta = direction * span;
  if (isMonthAligned(start, end)) {
    const newStart = yearMonthOfAbsMonth(absMonthIndex(start.year, start.month) + delta);
    const newEnd = yearMonthOfAbsMonth(absMonthIndex(end.year, end.month) + delta);
    return {
      start: new CalendarDate(newStart.year, newStart.month, 1),
      end: new CalendarDate(newEnd.year, newEnd.month, daysInMonth(newEnd.year, newEnd.month))
    };
  }
  return {
    start: shiftMonthsPreservingDay(start, delta),
    end: shiftMonthsPreservingDay(end, delta)
  };
}

/** True when the period starts at or before the book’s earliest month. */
export function isAtBookStart(start: CalendarDate, earliestDate: string | null): boolean {
  if (!earliestDate) return false;
  const year = Number(earliestDate.slice(0, 4));
  const month = Number(earliestDate.slice(5, 7));
  if (!Number.isInteger(year) || !Number.isInteger(month) || month < 1 || month > 12) return false;
  return absMonthIndex(start.year, start.month) <= absMonthIndex(year, month);
}

/**
 * True when stepping forward would leave the present: the period already ends
 * at/past the end of the current month, or it is the current year-to-date
 * window (ends today or later).
 */
export function isAtPresent(start: CalendarDate, end: CalendarDate): boolean {
  const now = new Date();
  const thisYear = now.getFullYear();
  const thisMonth = now.getMonth() + 1;
  const today = now.getDate();
  if (end.year !== thisYear) return end.year > thisYear;
  const monthEnd = daysInMonth(thisYear, end.month);
  if (end.month !== thisMonth) return end.month > thisMonth;
  if (end.day >= monthEnd) return true;
  return start.month === 1 && start.day === 1 && start.year === thisYear && end.day >= today;
}
