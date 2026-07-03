<script lang="ts">
  import * as Chart from '$lib/components/ui/chart';
  import { BarChart } from 'layerchart';
  import { formatMonth, formatCompact } from '$lib/format.svelte';
  import type { MonthBreakdown } from '$lib/server/dashboard-data';

  let { data }: { data: MonthBreakdown[] } = $props();

  let tickStep = $derived(Math.ceil(data.length / 6));

  let chartData = $derived(
    [...data].reverse().map((d) => ({
      month: d.month,
      income: d.income,
      expenses: d.expenses
    }))
  );

  const chartConfig = {
    income: {
      label: 'Income',
      color: 'var(--color-chart-1)'
    },
    expenses: {
      label: 'Expenses',
      color: 'var(--color-chart-2)'
    }
  } satisfies Chart.ChartConfig;
</script>

<Chart.Container config={chartConfig} class="min-h-62.5 w-full pl-2">
  <BarChart
    data={chartData}
    x="month"
    axis
    seriesLayout="group"
    bandPadding={0.3}
    series={[
      {
        key: 'income',
        label: chartConfig.income.label,
        color: chartConfig.income.color
      },
      {
        key: 'expenses',
        label: chartConfig.expenses.label,
        color: chartConfig.expenses.color
      }
    ]}
    props={{
      xAxis: {
        format: formatMonth,
        ticks: tickStep
      },
      yAxis: {
        format: formatCompact
      }
    }}
  >
    {#snippet tooltip()}
      <Chart.Tooltip />
    {/snippet}
  </BarChart>
</Chart.Container>
