<script lang="ts">
  import TimeSeriesChart from '$lib/components/shared/time-series-chart.svelte';
  import { sessionCurrency, parseLocalDate } from '$lib/utils/format.svelte';
  import type { NetWorthPoint } from '$lib/tauri';

  let { data, loading }: { data: NetWorthPoint[]; loading: boolean } = $props();

  let currency = $derived(sessionCurrency());
</script>

<!-- Month-end points: ranges shorter than a year would show one or two. -->
<TimeSeriesChart
  title="Net worth over time"
  description="At the end of each month, valued at that month’s prices"
  {loading}
  emptyMessage="Your net worth story starts with the first transaction."
  {data}
  getX={(d: NetWorthPoint) => parseLocalDate(d.date)}
  series={[{ key: 'netWorth', label: 'Net worth', color: 'var(--color-primary)' }]}
  {currency}
  tooltipTotals={[
    { label: 'Assets', compute: (d: NetWorthPoint) => d.assets },
    { label: 'Liabilities', compute: (d: NetWorthPoint) => d.liabilities }
  ]}
  ranges={['1Y', '5Y', 'All']}
/>
