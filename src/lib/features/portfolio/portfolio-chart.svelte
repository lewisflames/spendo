<script lang="ts">
  import TimeSeriesChart from '$lib/components/shared/time-series-chart.svelte';
  import { sessionCurrency } from '$lib/utils/format.svelte';
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
  description="Market value vs cost basis over time"
  {loading}
  emptyMessage="No performance data available."
  {data}
  getX={(d: PortfolioValuePoint) => new Date(d.date)}
  series={[
    { key: 'marketValue', label: 'Market Value', color: 'var(--color-success)' },
    { key: 'costBasis', label: 'Cost Basis', color: 'var(--color-warning)' }
  ]}
  {currency}
  {tooltipTotals}
/>
