/* eslint-disable svelte/prefer-svelte-reactivity -- Date instances are ephemeral, not stored in reactive state */
import { CalendarDate, parseDate } from '@internationalized/date';

export interface DatePreset {
  label: string;
  getRange: () => { start: CalendarDate; end: CalendarDate };
}

export function calendarDateToStr(cd: CalendarDate): string {
  // CalendarDate#toString() emits ISO `YYYY-MM-DD`.
  return cd.toString();
}

/** Local today as `YYYY-MM-DD` (single source for date defaults/guards). */
export function todayStr(): string {
  const d = new Date();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${d.getFullYear()}-${m}-${day}`;
}

/**
 * Shared reactive local "today". Read `today.value` from components so
 * "today"-dependent UI (due badges, date defaults, future-date guards) rolls
 * over at midnight instead of freezing on the date the page mounted.
 */
export const today = $state({ value: todayStr() });

function refreshToday(): void {
  const next = todayStr();
  if (today.value !== next) today.value = next;
}

// Browser-only wiring; vitest imports this module under Node, where neither
// `window` nor `document` exists.
if (typeof window !== 'undefined') {
  setInterval(refreshToday, 60_000);
  if (typeof document !== 'undefined') {
    document.addEventListener('visibilitychange', () => {
      if (!document.hidden) refreshToday();
    });
  }
}

/** Strict `YYYY-MM-DD` check with round-trip validation (rejects 2024-02-30). */
export function isIsoDate(value: string): boolean {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!m) return false;
  const y = Number(m[1]);
  const mo = Number(m[2]);
  const d = Number(m[3]);
  if (mo < 1 || mo > 12 || d < 1 || d > 31) return false;
  const dt = new Date(y, mo - 1, d);
  return dt.getFullYear() === y && dt.getMonth() === mo - 1 && dt.getDate() === d;
}

/** String-safe future check for `YYYY-MM-DD` (lexicographic = chronological). */
export function isFutureDate(value: string): boolean {
  return value > todayStr();
}

export function last12MonthsRange(): { start: CalendarDate; end: CalendarDate } {
  const now = new Date();
  const end = new Date(now.getFullYear(), now.getMonth(), 0);
  const start = new Date(end.getFullYear(), end.getMonth() - 11, 1);
  return {
    start: new CalendarDate(start.getFullYear(), start.getMonth() + 1, start.getDate()),
    end: new CalendarDate(end.getFullYear(), end.getMonth() + 1, end.getDate())
  };
}

export function computePrevRange(from: string, to: string): { prevFrom: string; prevTo: string } {
  const fromStr = from.slice(0, 10);
  const toStr = to.slice(0, 10);
  // Whole calendar years compare against the whole previous year (day-count
  // arithmetic would drift by a day across leap boundaries).
  const yearMatch = /^(\d{4})-01-01$/.exec(fromStr);
  if (yearMatch && toStr === `${yearMatch[1]}-12-31`) {
    const prevYear = Number(yearMatch[1]) - 1;
    return { prevFrom: `${prevYear}-01-01`, prevTo: `${prevYear}-12-31` };
  }
  const fromD = parseDate(fromStr);
  const toD = parseDate(toStr);
  // Same-length window immediately before `from` (whole-day arithmetic keeps
  // the previous period exactly as long as the current one).
  const lengthDays = toD.compare(fromD) + 1;
  const prevTo = fromD.subtract({ days: 1 });
  const prevFrom = prevTo.subtract({ days: lengthDays - 1 });
  return { prevFrom: calendarDateToStr(prevFrom), prevTo: calendarDateToStr(prevTo) };
}

/** Last day of a 1-based month (leap-year aware). */
export function daysInMonth(year: number, month: number): number {
  return new Date(year, month, 0).getDate();
}

/** Full calendar year: Jan 1 – Dec 31. */
export function getYearRange(year: number): { start: CalendarDate; end: CalendarDate } {
  return {
    start: new CalendarDate(year, 1, 1),
    end: new CalendarDate(year, 12, 31)
  };
}

/** Full calendar quarter (q is 1-4): first day of start month – last day of end month. */
export function getQuarterRange(
  year: number,
  q: number
): { start: CalendarDate; end: CalendarDate } {
  const quarter = Math.min(4, Math.max(1, Math.trunc(q)));
  const startMonth = (quarter - 1) * 3 + 1;
  const endMonth = startMonth + 2;
  return {
    start: new CalendarDate(year, startMonth, 1),
    end: new CalendarDate(year, endMonth, daysInMonth(year, endMonth))
  };
}

/** Full calendar month: 1st – last day. Month is 1-12, clamped. */
export function getMonthRange(
  year: number,
  month: number
): { start: CalendarDate; end: CalendarDate } {
  const m = Math.min(12, Math.max(1, Math.trunc(month)));
  return {
    start: new CalendarDate(year, m, 1),
    end: new CalendarDate(year, m, daysInMonth(year, m))
  };
}

/**
 * Span of whole calendar months from one month to another (inclusive).
 * Order-insensitive so range picks work in either direction and across years;
 * months are clamped like `getMonthRange`.
 */
export function getMonthRangeSpan(
  aYear: number,
  aMonth: number,
  bYear: number,
  bMonth: number
): { start: CalendarDate; end: CalendarDate } {
  const aY = Math.trunc(aYear);
  const bY = Math.trunc(bYear);
  const aM = Math.min(12, Math.max(1, Math.trunc(aMonth)));
  const bM = Math.min(12, Math.max(1, Math.trunc(bMonth)));
  const [fromY, fromM, toY, toM] =
    aY * 12 + aM <= bY * 12 + bM ? [aY, aM, bY, bM] : [bY, bM, aY, aM];
  return {
    start: new CalendarDate(fromY, fromM, 1),
    end: new CalendarDate(toY, toM, daysInMonth(toY, toM))
  };
}

/** Long month name for a 1-based month in the given locale. */
export function monthName(month: number, locale: string): string {
  return new Date(2000, month - 1, 1).toLocaleDateString(locale, { month: 'long' });
}

function getThisMonthRange(): { start: CalendarDate; end: CalendarDate } {
  const now = new Date();
  const start = new CalendarDate(now.getFullYear(), now.getMonth() + 1, 1);
  const endDate = new Date(now.getFullYear(), now.getMonth() + 1, 0).getDate();
  return { start, end: new CalendarDate(now.getFullYear(), now.getMonth() + 1, endDate) };
}

function getThisQuarterRange(): { start: CalendarDate; end: CalendarDate } {
  const now = new Date();
  return getQuarterRange(now.getFullYear(), Math.floor(now.getMonth() / 3) + 1);
}

function getThisYearRange(): { start: CalendarDate; end: CalendarDate } {
  return getYearRange(new Date().getFullYear());
}

/**
 * Fiscal year containing today for a 1-based start month. January behaves as
 * the calendar year; otherwise the window spans two calendar years (e.g.
 * start 4 → Apr 1 this year – Mar 31 next year when today is past April).
 */
function getFiscalYearContaining(
  fyStartMonth: number,
  now: Date = new Date()
): { start: CalendarDate; end: CalendarDate } {
  const m = Math.min(12, Math.max(1, Math.trunc(fyStartMonth) || 1));
  if (m === 1) return getYearRange(now.getFullYear());
  const year = now.getFullYear();
  const month = now.getMonth() + 1;
  const startYear = month >= m ? year : year - 1;
  const endYear = startYear + 1;
  const endMonth = m - 1;
  return {
    start: new CalendarDate(startYear, m, 1),
    end: new CalendarDate(endYear, endMonth, daysInMonth(endYear, endMonth))
  };
}

/** Resolve a stored P&L default period to a concrete range (FY-aware year). */
export function getPnlPeriodRange(
  period: string,
  fyStartMonth: number
): { start: CalendarDate; end: CalendarDate } {
  switch (period) {
    case 'this_month':
      return getThisMonthRange();
    case 'this_quarter':
      return getThisQuarterRange();
    case 'this_year':
      return getFiscalYearContaining(fyStartMonth);
    case 'last_12_months':
    default:
      return last12MonthsRange();
  }
}

export const PNL_PRESETS: DatePreset[] = [
  { label: 'This month', getRange: getThisMonthRange },
  { label: 'This quarter', getRange: getThisQuarterRange },
  { label: 'This year', getRange: getThisYearRange },
  { label: 'Last 12 months', getRange: last12MonthsRange }
];
