<script lang="ts" generics="T">
  import * as Card from '$lib/components/ui/card';
  import * as Chart from '$lib/components/ui/chart';
  import ChartTooltipRow from '$lib/components/ui/chart/chart-tooltip-row.svelte';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { AreaChart } from 'layerchart';
  import { formatCurrency, formatDate } from '$lib/utils/format.svelte';
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
    tooltipTotals
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
    dateRange === 'All' ? data : data.filter((d) => getX(d).getTime() >= rangeThreshold)
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

  const rangeOptions = [
    { value: '1M', label: '1M' },
    { value: '3M', label: '3M' },
    { value: '6M', label: '6M' },
    { value: '1Y', label: '1Y' },
    { value: '5Y', label: '5Y' },
    { value: 'All', label: 'All' }
  ] as const;
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
    <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between sm:gap-4">
      <div class="flex min-w-0 flex-col gap-1">
        <Card.Title class="truncate">{title}</Card.Title>
        {#if description}
          <Card.Description>{description}</Card.Description>
        {/if}
      </div>
      <div class="flex flex-wrap gap-1">
        {#each rangeOptions as opt (opt.value)}
          <button
            type="button"
            class="rounded-md px-2 py-1 text-xs font-medium transition-colors {dateRange ===
            opt.value
              ? 'bg-primary text-primary-foreground'
              : 'text-muted-foreground hover:bg-muted'}"
            aria-pressed={dateRange === opt.value}
            onclick={() => (dateRange = opt.value)}
          >
            {opt.label}
          </button>
        {/each}
      </div>
    </div>
  </Card.Header>
  <Card.Content>
    {#if loading}
      <div class="flex items-center justify-center">
        <Skeleton class="h-72 w-full" />
      </div>
    {:else if !data || data.length === 0}
      <div class="py-6 text-center text-sm text-muted-foreground">{emptyMessage}</div>
    {:else}
      <Chart.Container
        config={chartConfig satisfies Chart.ChartConfig}
        class="aspect-auto h-72 w-full"
      >
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
            }
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
      </Chart.Container>
    {/if}
  </Card.Content>
</Card.Root>
