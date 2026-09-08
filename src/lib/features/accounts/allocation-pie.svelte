<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Chart from '$lib/components/ui/chart';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { formatAllocPct } from '$lib/utils/format.svelte';
  import { PieChart } from 'layerchart';
  import ChartTooltipRow from '$lib/components/ui/chart/chart-tooltip-row.svelte';

  export interface AllocationDatum {
    key: string;
    label: string;
    value: number;
    color: string;
    /** Share of the total as `0-100`, for the tooltip. Null hides the pct. */
    sharePct?: number | null;
  }

  let {
    data,
    title = 'Allocation',
    description,
    centerLabel,
    centerValue,
    loading = false,
    emptyMessage = 'No balances found.',
    onSelect
  }: {
    data: AllocationDatum[];
    title?: string;
    description?: string;
    centerLabel?: string | null;
    centerValue?: string | null;
    loading?: boolean;
    emptyMessage?: string;
    /** Called with the datum key when a pie arc is clicked (e.g. drill-down). */
    onSelect?: (key: string) => void;
  } = $props();

  let chartConfig: Chart.ChartConfig = $derived.by(() => {
    const cfg: Chart.ChartConfig = {};
    for (const d of data) {
      cfg[d.key] = { label: d.label };
    }
    return cfg;
  });
</script>

<Card.Root>
  <Card.Header>
    <Card.Title>{title}</Card.Title>
    {#if description}
      <Card.Description>{description}</Card.Description>
    {/if}
  </Card.Header>
  <Card.Content>
    {#if loading}
      <Skeleton class="h-72 w-full" />
    {:else if data.length === 0}
      <div class="py-6 text-center text-sm text-muted-foreground">
        {emptyMessage}
      </div>
    {:else}
      <div class="relative">
        <Chart.Container config={chartConfig} class="aspect-auto h-72 w-full">
          <PieChart
            {data}
            key="key"
            label="label"
            value="value"
            innerRadius={0.6}
            cornerRadius={4}
            padAngle={0.035}
            c="color"
            onArcClick={onSelect ? (_, detail) => onSelect?.(String(detail.data.key)) : undefined}
          >
            {#snippet tooltip()}
              <Chart.Tooltip hideLabel>
                {#snippet formatter({ item })}
                  {@const datum = data.find((d) => d.key === item.key)}
                  <ChartTooltipRow
                    color={item.color as string}
                    label={datum?.label ?? String(item.label ?? item.key)}
                    value={datum?.sharePct != null ? formatAllocPct(datum.sharePct) : undefined}
                  />
                {/snippet}
              </Chart.Tooltip>
            {/snippet}
          </PieChart>
        </Chart.Container>
        {#if centerLabel || centerValue}
          <div class="pointer-events-none absolute inset-0 flex items-center justify-center">
            <div class="text-center">
              {#if centerLabel}
                <div class="text-xs text-muted-foreground">{centerLabel}</div>
              {/if}
              {#if centerValue}
                <div class="text-sm font-medium tabular-nums">{centerValue}</div>
              {/if}
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </Card.Content>
</Card.Root>
