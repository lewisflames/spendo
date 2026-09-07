<script lang="ts">
  import { TrendingUp, TrendingDown, ChartLine, Layers } from '@lucide/svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import { signColor, formatSessionCurrency as fmt, EMPTY_DASH } from '$lib/utils/format.svelte';
  import type { PortfolioHolding } from '$lib/tauri';

  let {
    holding: h
  }: {
    holding: PortfolioHolding;
  } = $props();
</script>

<!-- KPI summary cards -->
<div class="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-4">
  <KpiCard title="Market Value" valueClass="text-success">
    {#snippet icon()}
      <TrendingUp aria-hidden="true" class="size-4 text-success" />
    {/snippet}
    {#snippet value()}
      {fmt(h.marketValue)}
    {/snippet}
  </KpiCard>
  <KpiCard title="Gain / Loss" valueClass={signColor(h.gainLoss)}>
    {#snippet icon()}
      {#if h.gainLoss >= 0}
        <TrendingUp aria-hidden="true" class="size-4 text-success" />
      {:else}
        <TrendingDown aria-hidden="true" class="size-4 text-destructive" />
      {/if}
    {/snippet}
    {#snippet value()}
      {h.gainLoss >= 0 ? '+' : ''}{fmt(h.gainLoss)}
    {/snippet}
  </KpiCard>
  <KpiCard title="Current Price">
    {#snippet icon()}
      <ChartLine aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
    {#snippet value()}
      {h.price === null ? EMPTY_DASH : fmt(h.price)}
    {/snippet}
  </KpiCard>
  <KpiCard title="Shares">
    {#snippet icon()}
      <Layers aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
    {#snippet value()}
      {h.shares.toFixed(2)}
    {/snippet}
  </KpiCard>
</div>
