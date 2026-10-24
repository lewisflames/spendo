<script lang="ts" module>
  const ALL_RANGES = ['1M', '3M', '6M', '1Y', '5Y', 'All'] as const;
  export type ChartRange = (typeof ALL_RANGES)[number];
</script>

<script lang="ts" generics="T">
  import * as Card from '$lib/components/ui/card';
  import * as Chart from '$lib/components/ui/chart';
  import ChartTooltipRow from '$lib/components/ui/chart/chart-tooltip-row.svelte';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { AreaChart, BarChart } from 'layerchart';
  import {
    formatCurrency,
    formatDate,
    formatInteger,
    formatNumber
  } from '$lib/utils/format.svelte';
  import { curveStepAfter } from 'd3-shape';

  interface TimeSeriesSeries {
    key: string;
    label: string;
    color: string;
  }

  /** Derived row shown under the series rows in the tooltip (e.g. Gain/Loss). */
  type TimeSeriesTooltipTotal = {
    label: string;
    compute: (d: T) => number;
    color?: string;
  };

  let {
    title,
    description = '',
    loading,
    emptyMessage,
    data,
    getX,
    series,
    yField,
    currency,
    tooltipTotals,
    variant = 'area',
    showRangeFilter = true,
    ranges = ALL_RANGES
  }: {
    title: string;
    description?: string;
    loading: boolean;
    emptyMessage: string;
    data: T[];
    getX: (d: T) => Date;
    series: TimeSeriesSeries[];
    yField?: string;
    currency: string;
    tooltipTotals?: TimeSeriesTooltipTotal[];
    variant?: 'area' | 'bar';
    showRangeFilter?: boolean;
    /** Range buttons to offer; coarse (e.g. monthly) series drop the short ones. */
    ranges?: readonly ChartRange[];
  } = $props();

  let dateRange = $state('All');

  let rangeThreshold = $derived.by(() => {
    if (dateRange === 'All') return 0;
    const ms = {
      '1M': 30 * 24 * 60 * 60 * 1000,
      '3M': 90 * 24 * 60 * 60 * 1000,
      '6M': 180 * 24 * 60 * 60 * 1000,
      '1Y': 365 * 24 * 60 * 60 * 1000,
      '5Y': 5 * 365 * 24 * 60 * 60 * 1000
    }[dateRange]!;
    return Date.now() - ms;
  });

  let filteredData = $derived(
    showRangeFilter && dateRange !== 'All'
      ? data.filter((d) => getX(d).getTime() >= rangeThreshold)
      : data
  );

  let visibleSeries = $derived.by(() => {
    const filtered = series.filter((s) =>
      filteredData.some((d) => {
        const v = (d as Record<string, unknown>)[s.key];
        return typeof v === 'number' && v !== 0;
      })
    );
    return filtered.length > 0 ? filtered : series;
  });

  const chartConfig = $derived(
    Object.fromEntries(visibleSeries.map((s) => [s.key, { label: s.label, color: s.color }]))
  );

  // Locale-aware axis ticks: layerchart defaults to English month names, so
  // format explicitly with the app locale (cached formatters). X shows
  // localized "Mon Year" (e.g. Jan 2024 / janv. 2024); Y shows grouped
  // integers while the tooltip carries the full currency amount.
  function xTickFormatter(v: unknown): string {
    const d = v instanceof Date ? v : new Date(v as string | number);
    if (Number.isNaN(d.getTime())) return `${v ?? ''}`;
    return formatDate(d, { month: 'short', year: 'numeric' });
  }
  // Bar charts label every band, so "Mon Year" on each one collides; show the
  // year only on the first bar and on each January.
  function xBarTickFormatter(v: unknown): string {
    const d = v instanceof Date ? v : new Date(v as string | number);
    if (Number.isNaN(d.getTime())) return `${v ?? ''}`;
    const first = filteredData.length > 0 ? getX(filteredData[0]) : null;
    const isFirst =
      first !== null &&
      first.getFullYear() === d.getFullYear() &&
      first.getMonth() === d.getMonth();
    return d.getMonth() === 0 || isFirst
      ? formatDate(d, { month: 'short', year: 'numeric' })
      : formatDate(d, { month: 'short' });
  }
  function yTickFormatter(v: unknown): string {
    if (typeof v !== 'number' || !Number.isFinite(v)) return `${v ?? ''}`;
    // Keep axis labels honest for low-priced series (e.g. a price of 4.25):
    // only round to integers once the magnitude makes fractions noise.
    return Math.abs(v) >= 100 ? formatInteger(v) : formatNumber(v);
  }

  let rangeOptions = $derived(ranges.map((value) => ({ value, label: value })));
</script>

{#snippet tooltipFooter({ data }: { data: unknown })}
  {#each tooltipTotals ?? [] as total (total.label)}
    <ChartTooltipRow
      label={total.label}
      value={formatCurrency(total.compute(data as T), currency)}
      color={total.color}
    />
  {/each}
{/snippet}

<Card.Root>
  <Card.Header>
    <div class="flex flex-col gap-3 @xl:flex-row @xl:items-start @xl:justify-between @xl:gap-4">
      <div class="flex min-w-0 flex-col gap-1">
        <Card.Title class="truncate">{title}</Card.Title>
        {#if description}
          <Card.Description>{description}</Card.Description>
        {/if}
      </div>
      {#if showRangeFilter}
        <div class="flex flex-wrap gap-1" role="group" aria-label="Chart date range">
          {#each rangeOptions as opt (opt.value)}
            <button
              type="button"
              class="rounded-md px-2 py-1 text-xs font-medium transition-colors focus-visible:outline-2 focus-visible:outline-ring {dateRange ===
              opt.value
                ? 'bg-primary text-primary-foreground'
                : 'text-muted-foreground hover:bg-muted'}"
              aria-pressed={dateRange === opt.value}
              aria-label={opt.value === 'All' ? 'Show all dates' : `Show last ${opt.value}`}
              onclick={() => (dateRange = opt.value)}
            >
              {opt.label}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </Card.Header>
  <Card.Content>
    {#if loading}
      <div class="flex items-center justify-center">
        <Skeleton class="h-72 w-full" />
      </div>
    {:else if filteredData.length === 0}
      <div
        class="flex min-h-72 items-center justify-center py-6 text-center text-sm text-muted-foreground"
      >
        {emptyMessage}
      </div>
    {:else}
      <Chart.Container
        config={chartConfig satisfies Chart.ChartConfig}
        class="aspect-auto h-72 w-full"
      >
        {#if variant === 'bar'}
          <BarChart
            data={filteredData}
            x={getX}
            y={yField}
            seriesLayout="group"
            yNice={false}
            axis
            series={visibleSeries.map((s) => ({ key: s.key, label: s.label, color: s.color }))}
            padding={{ top: 0, right: 10, bottom: 20, left: 50 }}
            bandPadding={0.4}
            groupPadding={0.15}
            props={{
              xAxis: { format: xBarTickFormatter },
              yAxis: { format: yTickFormatter }
            }}
          >
            {#snippet tooltip()}
              <Chart.Tooltip
                labelFormatter={(v) => formatDate(v as string | Date)}
                valueFormatter={(v) => formatCurrency(v as number, currency)}
                footer={tooltipTotals ? tooltipFooter : undefined}
              />
            {/snippet}
          </BarChart>
        {:else}
          <AreaChart
            data={filteredData}
            x={getX}
            y={yField}
            seriesLayout="overlap"
            yBaseline={null}
            yNice={false}
            axis
            series={visibleSeries.map((s) => ({ key: s.key, label: s.label, color: s.color }))}
            padding={{ top: 0, right: 10, bottom: 20, left: 50 }}
            props={{
              area: {
                curve: curveStepAfter
              },
              xAxis: { format: xTickFormatter },
              yAxis: { format: yTickFormatter }
            }}
          >
            {#snippet tooltip()}
              <Chart.Tooltip
                labelFormatter={(v) => formatDate(v as string | Date)}
                valueFormatter={(v) => formatCurrency(v as number, currency)}
                footer={tooltipTotals ? tooltipFooter : undefined}
              />
            {/snippet}
          </AreaChart>
        {/if}
      </Chart.Container>
    {/if}
  </Card.Content>
</Card.Root>
