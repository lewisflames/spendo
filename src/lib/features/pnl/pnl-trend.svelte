<script lang="ts">
  import TimeSeriesChart from '$lib/components/shared/time-series-chart.svelte';
  import { sessionCurrency, parseLocalDate } from '$lib/utils/format.svelte';
  import type { PnlTrendPoint } from '$lib/tauri';

  let { data, loading }: { data: PnlTrendPoint[]; loading: boolean } = $props();

  let currency = $derived(sessionCurrency());
</script>

<TimeSeriesChart
  title="Income vs expenses"
  description="Monthly totals over the selected period"
  {loading}
  emptyMessage="No income or expense activity in this period."
  {data}
  getX={(d: PnlTrendPoint) => parseLocalDate(d.month)}
  series={[
    { key: 'income', label: 'Income', color: 'var(--color-success)' },
    { key: 'expenses', label: 'Expenses', color: 'var(--color-destructive)' }
  ]}
  {currency}
  tooltipTotals={[{ label: 'Net', compute: (d: PnlTrendPoint) => d.income - d.expenses }]}
  variant="bar"
/>
