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
  const fromD = parseDate(from.slice(0, 10));
  const toD = parseDate(to.slice(0, 10));
  // Same-length window immediately before `from` (whole-day arithmetic keeps
  // the previous period exactly as long as the current one).
  const lengthDays = toD.compare(fromD) + 1;
  const prevTo = fromD.subtract({ days: 1 });
  const prevFrom = prevTo.subtract({ days: lengthDays - 1 });
  return { prevFrom: calendarDateToStr(prevFrom), prevTo: calendarDateToStr(prevTo) };
}

function getThisMonthRange(): { start: CalendarDate; end: CalendarDate } {
  const now = new Date();
  const start = new CalendarDate(now.getFullYear(), now.getMonth() + 1, 1);
  const endDate = new Date(now.getFullYear(), now.getMonth() + 1, 0).getDate();
  return { start, end: new CalendarDate(now.getFullYear(), now.getMonth() + 1, endDate) };
}

function getPreviousMonthRange(): { start: CalendarDate; end: CalendarDate } {
  const now = new Date();
  const start = new Date(now.getFullYear(), now.getMonth() - 1, 1);
  const end = new Date(now.getFullYear(), now.getMonth(), 0);
  return {
    start: new CalendarDate(start.getFullYear(), start.getMonth() + 1, start.getDate()),
    end: new CalendarDate(end.getFullYear(), end.getMonth() + 1, end.getDate())
  };
}

function getThisYearRange(): { start: CalendarDate; end: CalendarDate } {
  const now = new Date();
  return {
    start: new CalendarDate(now.getFullYear(), 1, 1),
    end: new CalendarDate(now.getFullYear(), 12, 31)
  };
}

function getThisQuarterRange(): { start: CalendarDate; end: CalendarDate } {
  const now = new Date();
  const qStartMonth = Math.floor(now.getMonth() / 3) * 3;
  return {
    start: new CalendarDate(now.getFullYear(), qStartMonth + 1, 1),
    end: new CalendarDate(now.getFullYear(), now.getMonth() + 1, now.getDate())
  };
}

function getLastQuarterRange(): { start: CalendarDate; end: CalendarDate } {
  const now = new Date();
  const q = Math.floor(now.getMonth() / 3);
  if (q === 0) {
    return {
      start: new CalendarDate(now.getFullYear() - 1, 10, 1),
      end: new CalendarDate(now.getFullYear() - 1, 12, 31)
    };
  }
  const qStartMonth = (q - 1) * 3;
  const start = new CalendarDate(now.getFullYear(), qStartMonth + 1, 1);
  const endDate = new Date(now.getFullYear(), qStartMonth + 3, 0).getDate();
  return {
    start,
    end: new CalendarDate(now.getFullYear(), qStartMonth + 3, endDate)
  };
}

function getYearToDateRange(): { start: CalendarDate; end: CalendarDate } {
  const now = new Date();
  return {
    start: new CalendarDate(now.getFullYear(), 1, 1),
    end: new CalendarDate(now.getFullYear(), now.getMonth() + 1, now.getDate())
  };
}

function getLastYearRange(): { start: CalendarDate; end: CalendarDate } {
  const now = new Date();
  return {
    start: new CalendarDate(now.getFullYear() - 1, 1, 1),
    end: new CalendarDate(now.getFullYear() - 1, 12, 31)
  };
}

function getLast6MonthsRange(): { start: CalendarDate; end: CalendarDate } {
  const now = new Date();
  // Six complete calendar months, matching last12MonthsRange semantics:
  // end = last day of the previous month, start = first day five months back.
  const end = new Date(now.getFullYear(), now.getMonth(), 0);
  const start = new Date(end.getFullYear(), end.getMonth() - 5, 1);
  return {
    start: new CalendarDate(start.getFullYear(), start.getMonth() + 1, start.getDate()),
    end: new CalendarDate(end.getFullYear(), end.getMonth() + 1, end.getDate())
  };
}

export const PNL_PRESETS: DatePreset[] = [
  { label: 'This month', getRange: getThisMonthRange },
  { label: 'Last month', getRange: getPreviousMonthRange },
  { label: 'This quarter', getRange: getThisQuarterRange },
  { label: 'Year to date', getRange: getYearToDateRange },
  { label: 'Last quarter', getRange: getLastQuarterRange },
  { label: 'Last 6 months', getRange: getLast6MonthsRange },
  { label: 'This year', getRange: getThisYearRange },
  { label: 'Last year', getRange: getLastYearRange },
  { label: 'Last 12 months', getRange: last12MonthsRange }
];
