import { untrack } from 'svelte';
import type { DateRange } from 'bits-ui';
import { page } from '$app/state';
import { dbSession } from '$lib/stores/db.svelte';
import { loadBookSettings } from '$lib/stores/book-settings.svelte';
import { getPnlPeriodRange } from '$lib/utils/date-utils.svelte';

/**
 * Applies the book's default report period (FY-aware) once on first load
 * when the URL carries no explicit dates. Explicit `?from`/`?to` always win;
 * later user picks never get clobbered. Call after `useReportDateRange`.
 */
export function useBookDefaultPeriod(range: {
  get value(): DateRange;
  set value(v: DateRange);
  get dateFrom(): string;
  get dateTo(): string;
}): void {
  let applied = false;
  $effect(() => {
    if (!dbSession.value || applied) return;
    // Snapshot the URL-restored range: if the user picks a new period while
    // settings load, the snapshot differs and the default is skipped (the URL
    // guard alone loses the race because goto() updates the URL asynchronously).
    const rangeAtStart = untrack(() => `${range.dateFrom}|${range.dateTo}`);
    loadBookSettings()
      .then((settings) => {
        if (applied) return;
        applied = true;
        const sp = page.url.searchParams;
        if (sp.get('from') || sp.get('to') || `${range.dateFrom}|${range.dateTo}` !== rangeAtStart)
          return;
        const { start, end } = getPnlPeriodRange(
          settings.defaultPnlPeriod,
          settings.fiscalYearStartMonth
        );
        range.value = { start, end };
      })
      .catch(() => {
        applied = true;
      });
  });
}
