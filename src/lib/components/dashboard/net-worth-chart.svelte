<script lang="ts">
  import * as Chart from '$lib/components/ui/chart';
  import { AreaChart } from 'layerchart';
  import { formatMonth, formatCompact } from '$lib/format.svelte';
  import type { NetWorthPoint } from '$lib/server/dashboard-data';

  let { data }: { data: NetWorthPoint[] } = $props();

  let tickStep = $derived(Math.ceil(data.length / 6));

  const chartConfig = {
    netWorth: {
      label: 'Net Worth',
      color: 'var(--color-chart-1)'
    }
  } satisfies Chart.ChartConfig;
</script>

<Chart.Container config={chartConfig} class="min-h-55 w-full pl-4">
  <AreaChart
    {data}
    x="month"
    y="netWorth"
    axis
    series={[
      {
        key: 'netWorth',
        label: chartConfig.netWorth.label,
        color: chartConfig.netWorth.color
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
  </AreaChart>
</Chart.Container>
