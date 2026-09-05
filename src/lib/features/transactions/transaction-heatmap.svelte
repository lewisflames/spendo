<script lang="ts">
  import { timeWeek } from 'd3-time';
  import { scaleThreshold } from 'd3-scale';
  import { Calendar, Chart, Layer, Rect, Tooltip as ChartTooltip } from 'layerchart';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchTransactionHeatmap } from '$lib/tauri';
  import type { Filters, HeatmapPoint } from '$lib/tauri';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { formatDate, parseLocalDate, plural } from '$lib/utils/format.svelte';
  import * as Card from '$lib/components/ui/card';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import ErrorCard from '$lib/components/shared/error-card.svelte';

  let { filters }: { filters: Filters } = $props();

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    // Reads `filters`, so changing a filter refetches automatically.
    fetch: () => fetchTransactionHeatmap(filters),
    label: 'Failed to load activity'
  });

  let loading = $derived(res.loading);
  let error = $derived(res.error);

  interface CellDatum {
    date: Date;
    count: number;
  }

  let points = $derived(
    (res.data ?? []).map(
      (p: HeatmapPoint) => ({ date: parseLocalDate(p.date), count: p.count }) satisfies CellDatum
    )
  );

  // Week-grid bounds: the date filters when set, else the current year
  // (Jan 1 → Dec 31, so the grid extends past today). `end` is exclusive,
  // so the last day keeps its cell.
  let range = $derived.by(() => {
    const year = new Date().getFullYear();
    const start = filters.dateFrom ? parseLocalDate(filters.dateFrom) : new Date(year, 0, 1);
    const lastDay = filters.dateTo ? parseLocalDate(filters.dateTo) : new Date(year, 11, 31);
    const end = new Date(lastDay.getFullYear(), lastDay.getMonth(), lastDay.getDate() + 1);
    return { start, end, empty: start.getTime() >= lastDay.getTime() };
  });

  // Cells stay square and scale to fill the card width, capped so narrow
  // ranges can't blow them up. The chart height follows the cell size so
  // the 7-day grid always fits exactly.
  const CELL_CAP = 35;
  const TOP_PADDING = 20;
  const DAY_ROWS = 7;

  let chartWidth = $state(0);
  let cols = $derived(timeWeek.count(range.start, range.end) + 1);
  let cell = $derived(Math.min((chartWidth || 17.1) / cols, CELL_CAP));
  let chartHeight = $derived(Math.round(TOP_PADDING + DAY_ROWS * cell));

  // Intensity ramp from the accent chart color, tinted against the page
  // background so every level stays visibly colored in both themes.
  const CELL_RAMP = [
    'color-mix(in srgb, var(--color-chart-1) 25%, var(--color-background))',
    'color-mix(in srgb, var(--color-chart-1) 50%, var(--color-background))',
    'color-mix(in srgb, var(--color-chart-1) 75%, var(--color-background))',
    'var(--color-chart-1)'
  ];
</script>

<Card.Root>
  <Card.Header>
    <Card.Title>Activity</Card.Title>
    <Card.Description>Number of transactions per day</Card.Description>
  </Card.Header>
  <Card.Content>
    {#if error}
      <ErrorCard message={error} onRetry={() => res.reload()} />
    {:else if loading && !points.length}
      <Skeleton class="h-40 w-full" />
    {:else if range.empty}
      <div class="py-6 text-center text-sm text-muted-foreground">No activity in this period.</div>
    {:else}
      <div class="w-full" bind:clientWidth={chartWidth}>
        <Chart
          data={points}
          x={(d: CellDatum) => d.date}
          c={(d: CellDatum) => d.count}
          cScale={scaleThreshold().unknown('transparent')}
          cDomain={[1, 2, 4]}
          cRange={CELL_RAMP}
          padding={{ top: TOP_PADDING }}
          height={chartHeight}
        >
          {#snippet children({ context })}
            <Layer>
              <Calendar start={range.start} end={range.end} cellSize={[cell, cell]}>
                {#snippet children({ cells, cellSize })}
                  {@const gap = Math.min(3, Math.max(1, cellSize[0] * 0.12))}
                  {#each cells as cell (cell.data.date.getTime())}
                    <!-- Days without transactions keep a bordered placeholder
                       cell, so the grid reads as a full calendar. -->
                    <Rect
                      x={cell.x + gap / 2}
                      y={cell.y + gap / 2}
                      width={cellSize[0] - gap}
                      height={cellSize[1] - gap}
                      rx={4}
                      fill={cell.data.count != null ? cell.color : 'rgb(0 0 0 / 20%)'}
                      onpointermove={(e) => context.tooltip.show(e, cell.data)}
                      onpointerleave={() => context.tooltip.hide()}
                    />
                  {/each}
                {/snippet}
              </Calendar>
            </Layer>
            <ChartTooltip.Root variant="none">
              {#snippet children({ data: cell })}
                <div
                  class="flex gap-2 rounded-lg bg-popover p-2.5 text-xs text-popover-foreground shadow-md ring-1 ring-foreground/10"
                >
                  <div class="font-medium text-muted-foreground">{formatDate(cell.date)}</div>
                  <span class="font-medium tabular-nums"
                    >{plural(cell.count ?? 0, 'transaction')}</span
                  >
                </div>
              {/snippet}
            </ChartTooltip.Root>
          {/snippet}
        </Chart>
      </div>
    {/if}
  </Card.Content>
</Card.Root>
