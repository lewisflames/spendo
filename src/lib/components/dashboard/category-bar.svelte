<script lang="ts">
  import * as Chart from '$lib/components/ui/chart';
  import { PieChart } from 'layerchart';
  import type { TopCategory } from '$lib/server/dashboard-data';

  let { data }: { data: TopCategory[] } = $props();

  const palette = [
    'var(--color-chart-1)',
    'var(--color-chart-2)',
    'var(--color-chart-3)',
    'var(--color-chart-4)',
    'var(--color-chart-5)'
  ];

  const chartConfig = $derived(
    Object.fromEntries(
      data.map((d, i) => [d.name, { label: d.name, color: palette[i % palette.length] }])
    )
  ) satisfies Chart.ChartConfig;
</script>

<Chart.Container config={chartConfig} class="min-h-55 w-full">
  <PieChart
    {data}
    key="name"
    value="total"
    label="name"
    c="name"
    cRange={palette}
    innerRadius={0.4}
    cornerRadius={4}
    padAngle={0.02}
  >
    {#snippet tooltip()}
      <Chart.Tooltip />
    {/snippet}
  </PieChart>
</Chart.Container>
