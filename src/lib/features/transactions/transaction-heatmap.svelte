<script lang="ts">
  import { SvelteDate } from 'svelte/reactivity';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchTransactionHeatmap } from '$lib/tauri';
  import type { Filters, HeatmapPoint } from '$lib/tauri';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { formatDate, parseLocalDate, plural } from '$lib/utils/format.svelte';
  import * as Card from '$lib/components/ui/card';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import ErrorCard from '$lib/components/shared/error-card.svelte';

  let { filters }: { filters: Filters } = $props();

  // Mount the grid only once the browser is idle (or after two frames):
  // this keeps route navigation snappy by letting the ledger table win the
  // initial backend + render race. The card shell + skeleton paint immediately.
  let mounted = $state(false);

  $effect(() => {
    if (!dbSession.value) {
      mounted = false;
      return;
    }
    let cancelled = false;
    const show = () => {
      if (!cancelled) mounted = true;
    };
    const w = window as Window & {
      requestIdleCallback?: (cb: () => void, opts?: { timeout: number }) => number;
      cancelIdleCallback?: (id: number) => void;
    };
    let idleId: number | undefined;
    let raf = 0;
    if (typeof w.requestIdleCallback === 'function') {
      idleId = w.requestIdleCallback(show, { timeout: 800 });
    } else {
      raf = requestAnimationFrame(() => {
        raf = requestAnimationFrame(show);
      });
    }
    return () => {
      cancelled = true;
      if (idleId !== undefined) w.cancelIdleCallback?.(idleId);
      cancelAnimationFrame(raf);
    };
  });

  // Debounce filter commits independently of the ledger table (which uses a
  // shorter debounce): the heatmap is a full-scan GROUP BY, so typing must
  // not remount the grid at table frequency. The first commit is synchronous
  // so the initial fetch is not delayed. `lastKey` is plain (untracked) so
  // the effect only re-runs on filter changes, never on its own commits.
  let debouncedFilters = $state<Filters>({});
  let filtersCommitted = false;
  let lastKey = '';

  $effect(() => {
    const snapshot: Filters = {
      ...(filters.search ? { search: filters.search } : {}),
      ...(filters.dateFrom ? { dateFrom: filters.dateFrom } : {}),
      ...(filters.dateTo ? { dateTo: filters.dateTo } : {})
    };
    const key = JSON.stringify(snapshot);
    if (!filtersCommitted) {
      filtersCommitted = true;
      lastKey = key;
      debouncedFilters = snapshot;
      return;
    }
    if (key === lastKey) return;
    const timer = setTimeout(() => {
      lastKey = key;
      debouncedFilters = snapshot;
    }, 500);
    return () => clearTimeout(timer);
  });

  const res = useAsyncData({
    enabled: () => !!dbSession.value && mounted,
    // Reads `debouncedFilters`, so changing a filter refetches (debounced).
    fetch: () => fetchTransactionHeatmap(debouncedFilters),
    label: 'Failed to load activity'
  });

  // Backend dates are already `YYYY-MM-DD`, matching the grid keys below, so
  // no date parsing is needed for the counts lookup.
  let counts = $derived(new Map((res.data ?? []).map((p: HeatmapPoint) => [p.date, p.count])));

  // Week-grid bounds: the date filters when set, else the current year
  // (Jan 1 → Dec 31, so the grid extends past today). `end` is exclusive,
  // so the last day keeps its cell.
  let range = $derived.by(() => {
    const year = new Date().getFullYear();
    const start = debouncedFilters.dateFrom
      ? parseLocalDate(debouncedFilters.dateFrom)
      : new Date(year, 0, 1);
    const lastDay = debouncedFilters.dateTo
      ? parseLocalDate(debouncedFilters.dateTo)
      : new Date(year, 11, 31);
    const end = new Date(lastDay.getFullYear(), lastDay.getMonth(), lastDay.getDate() + 1);
    return { start, end, empty: start.getTime() >= lastDay.getTime() };
  });

  // Intensity ramp from the accent chart color, tinted against the page
  // background so every level stays visibly colored in both themes.
  // Thresholds mirror the previous threshold scale domain [1, 2, 4]: a day
  // with 1 transaction takes the second ramp step, 2–3 the third, 4+ the
  // full color. Days without transactions keep a neutral placeholder cell,
  // so the grid reads as a full calendar.
  const CELL_RAMP = [
    'color-mix(in srgb, var(--color-chart-1) 25%, var(--color-background))',
    'color-mix(in srgb, var(--color-chart-1) 50%, var(--color-background))',
    'color-mix(in srgb, var(--color-chart-1) 75%, var(--color-background))',
    'var(--color-chart-1)'
  ];
  const EMPTY_CELL = 'rgb(0 0 0 / 20%)';

  function toKey(d: Date): string {
    const m = `${d.getMonth() + 1}`.padStart(2, '0');
    const day = `${d.getDate()}`.padStart(2, '0');
    return `${d.getFullYear()}-${m}-${day}`;
  }

  function colorFor(count: number | null): string {
    if (count == null) return EMPTY_CELL;
    if (count >= 4) return CELL_RAMP[3];
    if (count >= 2) return CELL_RAMP[2];
    return CELL_RAMP[1];
  }

  interface DayCell {
    key: string;
    title: string;
    background: string;
    /** Padding days outside the selected range render dimmed. */
    dimmed: boolean;
  }

  interface WeekColumn {
    key: string;
    /** Short month name when the column enters a new month, else blank. */
    label: string;
    days: DayCell[];
  }

  // Sunday-start week columns (matching d3 `timeWeek`), padded to full weeks
  // so every column has 7 cells. Plain date arithmetic — no chart library.
  let columns = $derived.by((): WeekColumn[] => {
    const { start, end, empty } = range;
    if (empty) return [];
    const gridStart = new SvelteDate(start);
    gridStart.setDate(gridStart.getDate() - gridStart.getDay());
    const lastDay = new SvelteDate(end);
    lastDay.setDate(lastDay.getDate() - 1);
    const gridEnd = new SvelteDate(lastDay);
    gridEnd.setDate(gridEnd.getDate() + (6 - gridEnd.getDay()) + 1);

    const weekCols: WeekColumn[] = [];
    let current: DayCell[] = [];
    let prevMonth = -1;
    const cursor = new SvelteDate(gridStart);
    while (cursor.getTime() < gridEnd.getTime()) {
      const inRange = cursor.getTime() >= start.getTime() && cursor.getTime() < end.getTime();
      const key = toKey(cursor);
      const count = inRange ? (counts.get(key) ?? null) : null;
      current.push({
        key,
        title: inRange
          ? `${formatDate(key)}: ${plural(count ?? 0, 'transaction')}`
          : formatDate(key),
        background: colorFor(count),
        dimmed: !inRange
      });
      if (current.length === 7) {
        const firstInRange = current.find((d) => !d.dimmed);
        const month = firstInRange ? Number(firstInRange.key.slice(5, 7)) : prevMonth;
        const label =
          firstInRange && month !== prevMonth
            ? formatDate(firstInRange.key, { month: 'short' })
            : '';
        if (firstInRange) prevMonth = month;
        weekCols.push({ key: current[0].key, label, days: current });
        current = [];
      }
      cursor.setDate(cursor.getDate() + 1);
    }
    return weekCols;
  });

  // Strip-width cap (previous chart's 35px cell cap + 2px gap): fluid columns
  // never grow past this and center instead. Pure column count — no DOM
  // measurement, so nothing re-renders on window resize.
  let stripMaxWidth = $derived(columns.length * 37);
</script>

<Card.Root>
  <Card.Header>
    <Card.Title>Activity</Card.Title>
    <Card.Description>Number of transactions per day</Card.Description>
  </Card.Header>
  <Card.Content>
    {#if !mounted || (res.loading && columns.length === 0 && !res.error)}
      <Skeleton class="h-40 w-full" />
    {:else if res.error && columns.length === 0}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {:else if range.empty}
      <div class="py-6 text-center text-sm text-muted-foreground">No activity in this period.</div>
    {:else}
      {#if res.error}
        <div class="mb-3">
          <ErrorCard message={res.error} onRetry={() => res.reload()} />
        </div>
      {/if}
      <div
        class="mx-auto w-full transition-opacity {res.loading
          ? 'pointer-events-none opacity-50'
          : ''}"
        style={`max-width: ${stripMaxWidth}px;`}
      >
        <div role="img" aria-label="Number of transactions per day" class="flex gap-0.5">
          {#each columns as week (week.key)}
            <div class="flex min-w-0 flex-1 flex-col gap-0.5">
              <div class="h-4 text-[10px] leading-4 text-muted-foreground" aria-hidden="true">
                {week.label}
              </div>
              {#each week.days as day (day.key)}
                <div
                  class="aspect-square w-full rounded-[4px] border border-foreground/10 {day.dimmed
                    ? 'opacity-40'
                    : ''}"
                  style={`background: ${day.background};`}
                  title={day.title}
                ></div>
              {/each}
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </Card.Content>
</Card.Root>
