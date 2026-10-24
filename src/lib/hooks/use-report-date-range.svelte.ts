import { CalendarDate } from '@internationalized/date';
import type { DateRange } from 'bits-ui';
import { calendarDateToStr, last12MonthsRange } from '$lib/utils/date-utils.svelte';
import { useUrlParams } from './use-url-params.svelte';

function applyCalendarParam(
  getRange: () => DateRange,
  setRange: (range: DateRange) => void,
  value: string | null,
  side: 'start' | 'end'
) {
  if (!value) return;
  const [y, m, d] = value.split('-').map(Number);
  if (!Number.isInteger(y) || !Number.isInteger(m) || !Number.isInteger(d)) return;
  if (m < 1 || m > 12 || d < 1 || d > 31) return;
  const date = new CalendarDate(y, m, d);
  const current = getRange();
  setRange({
    start: side === 'start' ? date : (current?.start ?? date),
    end: side === 'end' ? date : (current?.end ?? date)
  });
}

/**
 * Shared period state for the P&L and spending reports: calendar range bound
 * to `DateRangePicker`, `YYYY-MM-DD` derivations, inverted-range guard, and
 * `?from&?to` URL sync. Must be called at component top level before
 * `useAsyncData` so the first fetch already uses URL-restored dates.
 */
export function useReportDateRange(): {
  get value(): DateRange;
  set value(v: DateRange);
  get dateFrom(): string;
  get dateTo(): string;
  get rangeValid(): boolean;
} {
  let calendarRange = $state<DateRange>(last12MonthsRange());

  const dateFrom = $derived(
    calendarRange?.start ? calendarDateToStr(calendarRange.start as CalendarDate) : ''
  );
  const dateTo = $derived(
    calendarRange?.end ? calendarDateToStr(calendarRange.end as CalendarDate) : ''
  );
  // YYYY-MM-DD strings compare lexicographically; an inverted range would
  // silently yield zeros from the backend, so callers block the fetch.
  const rangeValid = $derived(!dateFrom || !dateTo || dateFrom <= dateTo);

  const getRange = () => calendarRange;
  const setRange = (range: DateRange) => {
    calendarRange = range;
  };

  useUrlParams([
    {
      key: 'from',
      get: () => dateFrom || null,
      apply: (v) => applyCalendarParam(getRange, setRange, v, 'start'),
      replace: true
    },
    {
      key: 'to',
      get: () => dateTo || null,
      apply: (v) => applyCalendarParam(getRange, setRange, v, 'end'),
      replace: true
    }
  ]);

  return {
    get value() {
      return calendarRange;
    },
    set value(v: DateRange) {
      calendarRange = v;
    },
    get dateFrom() {
      return dateFrom;
    },
    get dateTo() {
      return dateTo;
    },
    get rangeValid() {
      return rangeValid;
    }
  };
}
