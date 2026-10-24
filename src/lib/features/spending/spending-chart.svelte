<script lang="ts">
  import TimeSeriesChart from '$lib/components/shared/time-series-chart.svelte';
  import { sessionCurrency, parseLocalDate } from '$lib/utils/format.svelte';
  import type { SpendingReport } from '$lib/tauri';
  import { buildSpendingRows, type SpendingChartRow, type SpendingSeries } from './spending-utils';

  let {
    report,
    series,
    loading
  }: { report: SpendingReport | null; series: SpendingSeries[]; loading: boolean } = $props();

  let currency = $derived(sessionCurrency());
  let rows = $derived(report && series.length > 0 ? buildSpendingRows(report, series) : []);
</script>

{#snippet legend()}
  {#if series.length > 1}
    <ul class="mt-4 flex flex-wrap gap-x-4 gap-y-1.5 text-xs text-muted-foreground">
      {#each series as s (s.key)}
        <li class="flex min-w-0 items-center gap-1.5">
          <span
            aria-hidden="true"
            class="size-2.5 shrink-0 rounded-[3px]"
            style:background-color={s.color}
          ></span>
          <span class="truncate">{s.label}</span>
        </li>
      {/each}
    </ul>
  {/if}
{/snippet}

<TimeSeriesChart
  title="Spending by month"
  description="Each bar split by category"
  {loading}
  emptyMessage="Nothing spent in this period. Your wallet says thanks."
  data={rows}
  getX={(d: SpendingChartRow) => parseLocalDate(d.month)}
  series={series.map((s) => ({ key: s.key, label: s.label, color: s.color }))}
  {currency}
  tooltipTotals={[
    {
      label: 'Total',
      compute: (d: SpendingChartRow) => series.reduce((sum, s) => sum + (Number(d[s.key]) || 0), 0)
    }
  ]}
  variant="bar"
  stacked
  showRangeFilter={false}
  footer={legend}
/>
