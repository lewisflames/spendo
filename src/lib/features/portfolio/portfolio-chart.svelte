<script lang="ts">
  import TimeSeriesChart from '$lib/components/shared/time-series-chart.svelte';
  import { sessionCurrency, parseLocalDate } from '$lib/utils/format.svelte';
  import type { PortfolioValuePoint } from '$lib/tauri';

  let { data, loading }: { data: PortfolioValuePoint[]; loading: boolean } = $props();

  let currency = $derived(sessionCurrency());

  let tooltipTotals = [
    {
      label: 'Gain / Loss',
      compute: (d: PortfolioValuePoint) => d.marketValue - d.costBasis
    }
  ];
</script>

<TimeSeriesChart
  title="Portfolio Performance"
  description="What it’s worth versus what you paid, over time"
  {loading}
  emptyMessage="Not enough history to chart yet."
  {data}
  getX={(d: PortfolioValuePoint) => parseLocalDate(d.date)}
  series={[
    { key: 'marketValue', label: 'Market Value', color: 'var(--color-success)' },
    { key: 'costBasis', label: 'Cost Basis', color: 'var(--color-warning)' }
  ]}
  {currency}
  {tooltipTotals}
/>
