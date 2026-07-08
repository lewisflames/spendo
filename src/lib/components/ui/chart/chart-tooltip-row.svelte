<script lang="ts">
  import { cn, type WithElementRef, type WithoutChildren } from '$lib/utils.js';
  import type { HTMLAttributes } from 'svelte/elements';
  import type { Snippet } from 'svelte';

  let {
    ref = $bindable(null),
    class: className,
    color,
    icon,
    indicator = 'dot',
    label,
    value,
    valueSuffix,
    valueClass,
    ...restProps
  }: WithoutChildren<WithElementRef<HTMLAttributes<HTMLDivElement>>> & {
    /** Fill color of the indicator. Ignored when `icon` is set or `indicator` is `none`. */
    color?: string;
    /** Optional icon rendered in place of the color indicator. */
    icon?: Snippet;
    indicator?: 'dot' | 'line' | 'dashed' | 'none';
    label: string;
    value?: string | number;
    /** Muted suffix rendered after the value (e.g. a percentage). */
    valueSuffix?: string;
    valueClass?: string;
  } = $props();
</script>

<!--
  Fixed-width first column keeps indicator/label columns aligned across sibling
  rows; `items-baseline` aligns label/value text baselines despite mixed fonts.
-->
<div
  bind:this={ref}
  class={cn('grid grid-cols-[0.625rem_1fr_auto] items-baseline gap-x-2.5', className)}
  {...restProps}
>
  {#if icon}
    {@render icon()}
  {:else if indicator === 'line'}
    <span class="h-3.5 self-center rounded-[2px]" style="background-color: {color};"></span>
  {:else if indicator === 'dashed'}
    <span
      class="h-3.5 self-center rounded-[2px] border-l-[1.5px] border-dashed"
      style="border-color: {color};"
    ></span>
  {:else if indicator === 'dot' && color}
    <span class="size-2.5 self-center rounded-[2px]" style="background-color: {color};"></span>
  {:else}
    <span></span>
  {/if}
  <span class="truncate text-muted-foreground">{label}</span>
  {#if value !== undefined && value !== null}
    <span class={cn('font-medium tabular-nums', valueClass)}>
      {value}{#if valueSuffix}<span class="ml-1 font-normal text-muted-foreground"
          >{valueSuffix}</span
        >{/if}
    </span>
  {/if}
</div>
