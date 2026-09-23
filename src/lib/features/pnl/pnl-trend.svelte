<script lang="ts">
  import TimeSeriesChart from '$lib/components/shared/time-series-chart.svelte';
  import { sessionCurrency, parseLocalDate } from '$lib/utils/format.svelte';
  import type { PnlTrendPoint } from '$lib/tauri';

  let { data, loading }: { data: PnlTrendPoint[]; loading: boolean } = $props();

  let currency = $derived(sessionCurrency());
</script>

<TimeSeriesChart
  title="Income vs expenses"
  description="Month by month, for the stretch you picked"
  {loading}
  emptyMessage="Quiet stretch. Nothing earned or spent here."
  {data}
  getX={(d: PnlTrendPoint) => parseLocalDate(d.month)}
  series={[
    { key: 'income', label: 'Income', color: 'var(--color-success)' },
    { key: 'expenses', label: 'Expenses', color: 'var(--color-destructive)' }
  ]}
  {currency}
  tooltipTotals={[{ label: 'Net', compute: (d: PnlTrendPoint) => d.income - d.expenses }]}
  variant="bar"
  showRangeFilter={false}
/>
