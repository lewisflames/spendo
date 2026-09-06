<script lang="ts">
  import { Briefcase, TrendingUp, TrendingDown, LineChart, Tag } from '@lucide/svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import { signColor, fmtPct, formatSessionCurrency as fmt } from '$lib/utils/format.svelte';
  import {
    STALE_PRICE_DAYS,
    priceStaleDays,
    dayChangePct,
    range52w
  } from '$lib/features/portfolio/price-utils';
  import { formatDate } from '$lib/utils/format.svelte';
  import type { PortfolioHolding, PriceHistoryEntry } from '$lib/tauri';

  let {
    holding: h,
    priceHistory,
    priceCurrency
  }: {
    holding: PortfolioHolding;
    priceHistory: PriceHistoryEntry[];
    priceCurrency: string;
  } = $props();

  let range = $derived(range52w(priceHistory));
</script>

<!-- KPI summary cards -->
<div class="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-3">
  <KpiCard title="Market Value" value={fmt(h.marketValue)}>
    {#snippet icon()}
      <Briefcase aria-hidden="true" class="size-4 text-muted-foreground" />
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
      <span class="text-sm font-normal">
        ({h.costBasis !== 0 ? fmtPct(h.gainLossPct) : '—'})
      </span>
    {/snippet}
  </KpiCard>
  <KpiCard title="Current Price" valueClass={signColor(dayChangePct(h) ?? 0)}>
    {#snippet icon()}
      <LineChart aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
    {#snippet value()}
      {h.price === null ? '—' : fmt(h.price)}
      <span class="text-xs font-normal text-muted-foreground">
        {#if h.priceDate}
          {@const stale = priceStaleDays(h.priceDate) > STALE_PRICE_DAYS}
          {formatDate(h.priceDate)}{stale ? ' · stale' : ''}
        {/if}
        {#if dayChangePct(h) !== null}
          · {fmtPct(dayChangePct(h))}
        {/if}
      </span>
    {/snippet}
  </KpiCard>
  <KpiCard title="Shares">
    {#snippet icon()}
      <Tag aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
    {#snippet value()}
      {h.shares.toFixed(2)}
      <span class="text-sm font-normal text-muted-foreground">
        avg {h.shares > 0 ? fmt(h.avgCostPerShare) : '—'}
      </span>
    {/snippet}
  </KpiCard>
  <KpiCard title="52W Range" valueClass="text-muted-foreground">
    {#snippet icon()}
      <LineChart aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
    {#snippet value()}
      {#if range}
        <span class="tabular-nums">
          {range.low.toFixed(2)} – {range.high.toFixed(2)}
          {priceCurrency}
        </span>
      {:else}
        —
      {/if}
    {/snippet}
  </KpiCard>
  <KpiCard title="Allocation" value={h.allocationPct > 0 ? `${h.allocationPct.toFixed(1)}%` : '—'}>
    {#snippet icon()}
      <Briefcase aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </KpiCard>
</div>
