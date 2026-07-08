<script lang="ts">
  import { cn, type WithElementRef, type WithoutChildren } from '$lib/utils.js';
  import type { HTMLAttributes } from 'svelte/elements';
  import { getPayloadConfigFromPayload, useChart, type TooltipPayload } from './chart-utils.js';
  import { getChartContext, Tooltip as TooltipPrimitive } from 'layerchart';
  import ChartTooltipRow from './chart-tooltip-row.svelte';
  import type { Snippet } from 'svelte';

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  function defaultFormatter(value: any, _payload: TooltipPayload[]) {
    return `${value}`;
  }

  function resolveItemColor(
    itemConfig: { color?: string; theme?: Record<string, string> } | undefined
  ): string | undefined {
    if (!itemConfig) return;
    if (itemConfig.color) return itemConfig.color;
    if (itemConfig.theme) {
      const isDark =
        typeof document !== 'undefined' && document.documentElement.classList.contains('dark');
      return itemConfig.theme[isDark ? 'dark' : 'light'];
    }
  }

  let {
    ref = $bindable(null),
    class: className,
    hideLabel = false,
    indicator = 'dot',
    hideIndicator = false,
    labelKey,
    label,
    labelFormatter = defaultFormatter,
    labelClassName,
    formatter,
    nameKey,
    color,
    valueFormatter,
    footer,
    ...restProps
  }: WithoutChildren<WithElementRef<HTMLAttributes<HTMLDivElement>>> & {
    hideLabel?: boolean;
    label?: string;
    indicator?: 'dot' | 'line' | 'dashed';
    nameKey?: string;
    labelKey?: string;
    hideIndicator?: boolean;
    labelClassName?: string;
    labelFormatter?:
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      ((value: any, payload: TooltipPayload[]) => string | number | Snippet) | null;
    formatter?: Snippet<
      [
        {
          value: unknown;
          name: string;
          item: TooltipPayload;
          index: number;
          payload: TooltipPayload[];
        }
      ]
    >;
    valueFormatter?: (value: unknown) => string;
    /** Rendered below the series rows, separated by a hairline (e.g. totals). */
    footer?: Snippet<[{ data: unknown }]>;
  } = $props();

  const chart = useChart();
  const chartCtx = getChartContext();

  // Filter to series with defined values (important for item-based charts like Pie/Arc
  // where only the hovered item has a value)
  const visibleSeries = $derived(
    chartCtx.tooltip.series.filter((s: TooltipPayload) => s.value !== undefined)
  );

  const formattedLabel = $derived.by(() => {
    if (hideLabel || !visibleSeries?.length) return null;

    const [item] = visibleSeries;
    const tooltipData = chartCtx.tooltip.data;

    // Get the x-axis label value from the raw tooltip data (e.g. a Date or month string)
    const dataLabel = tooltipData != null ? chartCtx.x(tooltipData) : undefined;

    const key = labelKey ?? item?.label ?? item?.key ?? 'value';
    const itemConfig = getPayloadConfigFromPayload(
      chart.config,
      item,
      key,
      tooltipData as Record<string, unknown> | null
    );

    let value: unknown;
    if (!labelKey && typeof label === 'string') {
      value = chart.config[label as keyof typeof chart.config]?.label ?? label;
    } else if (labelKey) {
      value = itemConfig?.label ?? dataLabel;
    } else {
      value = dataLabel;
    }

    if (value === undefined) return null;
    if (!labelFormatter) return value;
    return labelFormatter(value, visibleSeries);
  });
</script>

{#snippet TooltipLabel()}
  {#if formattedLabel}
    <div class={cn('font-medium', labelClassName)}>
      {#if typeof formattedLabel === 'function'}
        {@render formattedLabel()}
      {:else}
        {formattedLabel}
      {/if}
    </div>
  {/if}
{/snippet}

<TooltipPrimitive.Root variant="none">
  <div
    bind:this={ref}
    class={cn(
      'grid min-w-[10rem] gap-2 rounded-lg bg-popover p-2.5 text-xs text-popover-foreground shadow-md ring-1 ring-foreground/10',
      className
    )}
    {...restProps}
  >
    {@render TooltipLabel()}
    <div class="grid gap-1.5">
      {#each visibleSeries as item, i (item.key + i)}
        {@const key = `${nameKey || item.key || item.label || 'value'}`}
        {@const itemConfig = getPayloadConfigFromPayload(
          chart.config,
          item,
          key,
          chartCtx.tooltip.data
        )}
        {@const indicatorColor =
          color ||
          resolveItemColor(itemConfig) ||
          item.config?.color ||
          item.color ||
          `var(--color-${key})`}
        {#if formatter && item.value !== undefined && item.label}
          {@render formatter({
            value: item.value,
            name: item.label,
            item,
            index: i,
            payload: visibleSeries
          })}
        {:else if itemConfig?.icon}
          {#snippet rowIcon()}
            <itemConfig.icon class="size-2.5 text-muted-foreground" />
          {/snippet}
          <ChartTooltipRow icon={rowIcon} label={itemConfig?.label || item.label || key} />
        {:else}
          <ChartTooltipRow
            {indicator}
            color={hideIndicator ? undefined : indicatorColor}
            label={itemConfig?.label || item.label || key}
            value={item.value !== undefined
              ? valueFormatter
                ? valueFormatter(item.value)
                : item.value.toLocaleString()
              : undefined}
          />
        {/if}
      {/each}
    </div>
    {#if footer}
      <div class="grid gap-1.5 border-t border-border/60 pt-1.5">
        {@render footer({ data: chartCtx.tooltip.data })}
      </div>
    {/if}
  </div>
</TooltipPrimitive.Root>
