<script lang="ts">
  import { TrendingUp, TrendingDown, ChartLine, Layers } from '@lucide/svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import {
    signColor,
    formatSessionCurrency as fmt,
    formatSignedCurrency,
    sessionCurrency,
    formatShares,
    formatCurrency,
    EMPTY_DASH
  } from '$lib/utils/format.svelte';
  import type { PortfolioHolding } from '$lib/tauri';

  let {
    holding: h,
    priceCurrency = null
  }: {
    holding: PortfolioHolding;
    priceCurrency?: string | null;
  } = $props();

  let currency = $derived(sessionCurrency());
</script>

<!-- KPI summary cards -->
<div class="grid grid-cols-1 gap-6 @xl:grid-cols-2 @4xl:grid-cols-4">
  <KpiCard title="Market value">
    {#snippet icon()}
      <TrendingUp aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
    {#snippet value()}
      {fmt(h.marketValue)}
    {/snippet}
  </KpiCard>
  <KpiCard title="Gain / loss" valueClass={signColor(h.gainLoss)}>
    {#snippet icon()}
      {#if h.gainLoss >= 0}
        <TrendingUp aria-hidden="true" class="size-4 text-success" />
      {:else}
        <TrendingDown aria-hidden="true" class="size-4 text-destructive" />
      {/if}
    {/snippet}
    {#snippet value()}
      {formatSignedCurrency(h.gainLoss, currency)}
    {/snippet}
  </KpiCard>
  <KpiCard title="Current price">
    {#snippet icon()}
      <ChartLine aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
    {#snippet value()}
      {h.price === null ? EMPTY_DASH : formatCurrency(h.price, priceCurrency ?? currency)}
    {/snippet}
  </KpiCard>
  <KpiCard title="Shares">
    {#snippet icon()}
      <Layers aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
    {#snippet value()}
      {formatShares(h.shares)}
    {/snippet}
  </KpiCard>
</div>
