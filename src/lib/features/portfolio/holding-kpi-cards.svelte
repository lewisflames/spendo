<script lang="ts">
  import { TrendingUp, TrendingDown, HandCoins, Coins } from '@lucide/svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import {
    signColor,
    formatSessionCurrency as fmt,
    formatSignedCurrency,
    sessionCurrency,
    EMPTY_DASH
  } from '$lib/utils/format.svelte';
  import type { PortfolioHolding } from '$lib/tauri';

  let { holding: h }: { holding: PortfolioHolding } = $props();

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
  <KpiCard
    title="Unrealized gain"
    valueClass={h.unrealizedGain === null ? 'text-muted-foreground' : signColor(h.unrealizedGain)}
  >
    {#snippet icon()}
      {#if h.unrealizedGain !== null && h.unrealizedGain < 0}
        <TrendingDown aria-hidden="true" class="size-4 text-destructive" />
      {:else}
        <TrendingUp
          aria-hidden="true"
          class="size-4 {h.unrealizedGain === null ? 'text-muted-foreground' : 'text-success'}"
        />
      {/if}
    {/snippet}
    {#snippet value()}
      {h.unrealizedGain === null ? EMPTY_DASH : formatSignedCurrency(h.unrealizedGain, currency)}
    {/snippet}
  </KpiCard>
  <KpiCard title="All-time realized gain" valueClass={signColor(h.realizedGain)}>
    {#snippet icon()}
      <HandCoins aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
    {#snippet value()}
      {formatSignedCurrency(h.realizedGain, currency)}
    {/snippet}
  </KpiCard>
  <KpiCard title="All-time dividends">
    {#snippet icon()}
      <Coins aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
    {#snippet value()}
      {fmt(h.dividends)}
    {/snippet}
  </KpiCard>
</div>
