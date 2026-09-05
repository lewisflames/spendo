<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { Briefcase, TrendingUp, TrendingDown, RefreshCw } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchPortfolioData, refreshPrices } from '$lib/tauri';
  import {
    signColor,
    sessionCurrency,
    formatSessionCurrency,
    formatDate
  } from '$lib/utils/format.svelte';
  import { errMsg } from '$lib/utils/error';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { Button } from '$lib/components/ui/button';
  import { toast } from '$lib/components/ui/sonner';
  import PortfolioChart from '$lib/features/portfolio/portfolio-chart.svelte';
  import HoldingsTable from '$lib/features/portfolio/holdings-table.svelte';
  import AllocationPie from '$lib/features/accounts/allocation-pie.svelte';
  import { holdingLabel } from '$lib/features/portfolio/price-utils';
  import { gotoHolding } from '$lib/utils/navigation';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { plural } from '$lib/utils/format.svelte';

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: fetchPortfolioData,
    label: 'Failed to load portfolio'
  });

  let loading = $derived(res.loading);

  let currency = $derived(sessionCurrency());

  let refreshingPrices = $state(false);

  async function handleRefreshPrices() {
    if (!dbSession.value || refreshingPrices) return;
    refreshingPrices = true;
    try {
      const result = await refreshPrices();
      if (result.errors.length > 0) {
        toast.warning(`Prices refreshed with ${plural(result.errors.length, 'error')}`);
      } else {
        toast.success('Prices refreshed');
      }
      // Reload so market values, day change and the timestamp pick up the new prices.
      res.reload();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to refresh prices'));
    } finally {
      refreshingPrices = false;
    }
  }

  let portfolioColors = [
    'var(--color-chart-1)',
    'var(--color-chart-2)',
    'var(--color-chart-3)',
    'var(--color-chart-4)',
    'var(--color-chart-5)'
  ];

  function holdingColor(index: number): string {
    return portfolioColors[index % portfolioColors.length];
  }

  let pieData = $derived.by(() => {
    if (!res.data) return [];
    const total = res.data.holdings.reduce((s, h) => s + Math.max(h.marketValue, 0), 0);
    return res.data.holdings.map((h, i) => ({
      key: h.commodityGuid,
      label: holdingLabel(h),
      value: Math.max(h.marketValue, 0),
      color: holdingColor(i),
      sharePct: total > 0 ? (Math.max(h.marketValue, 0) / total) * 100 : null
    }));
  });

  let headerDescription = $derived.by(() => {
    let desc = 'Investment holdings overview';
    const lastRefreshed = res.data?.lastRefreshed;
    if (lastRefreshed) {
      desc += ` · Prices updated ${formatDate(lastRefreshed, {
        dateStyle: 'medium',
        timeStyle: 'short'
      })}`;
    }
    return desc;
  });

  const fmt = formatSessionCurrency;
</script>

<svelte:head>
  <title>Portfolio — Spendo</title>
  <meta name="description" content="Investment holdings overview." />
</svelte:head>

<div class="flex flex-col gap-6">
  <PageHeader title="Portfolio" description={headerDescription}>
    {#if dbSession.value}
      <div class="flex items-center gap-2">
        <Button onclick={handleRefreshPrices} disabled={loading || refreshingPrices}>
          <RefreshCw aria-hidden="true" class="size-3.5 {refreshingPrices ? 'animate-spin' : ''}" />
          {refreshingPrices ? 'Refreshing...' : 'Refresh Prices'}
        </Button>
      </div>
    {/if}
  </PageHeader>

  {#if !dbSession.value}
    <ImportPrompt />
  {:else if loading && !res.data}
    <!-- KPI skeletons -->
    <KpiCardRow />
    <!-- Chart skeleton -->
    <Card.Root>
      <Card.Content>
        <Skeleton class="h-72 w-full" />
      </Card.Content>
    </Card.Root>
    <HoldingsTable loading {currency} holdings={[]} />
  {:else if res.error}
    <ErrorCard message={res.error} onRetry={() => res.reload()} />
  {:else if res.data}
    {@const d = res.data}

    {#if d.holdings.length > 0}
      <!-- KPI summary cards -->
      <div class="grid grid-cols-1 gap-6 sm:grid-cols-3">
        <KpiCard title="Market Value" value={fmt(d.totalMarketValue)} valueClass="text-success">
          {#snippet icon()}
            <TrendingUp aria-hidden="true" class="size-4 text-success" />
          {/snippet}
        </KpiCard>
        <KpiCard title="Cost Basis" value={fmt(d.totalCostBasis)} valueClass="text-warning">
          {#snippet icon()}
            <Briefcase aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
        <KpiCard title="Gain / Loss" valueClass={signColor(d.totalGainLoss)}>
          {#snippet icon()}
            {#if d.totalGainLoss >= 0}
              <TrendingUp aria-hidden="true" class="size-4 text-success" />
            {:else}
              <TrendingDown aria-hidden="true" class="size-4 text-destructive" />
            {/if}
          {/snippet}
          {#snippet value()}
            {d.totalGainLoss >= 0 ? '+' : ''}{fmt(d.totalGainLoss)}
            <span class="text-sm font-normal">
              ({d.totalGainLoss >= 0 ? '+' : ''}{d.totalGainLossPct.toFixed(1)}%)
            </span>
          {/snippet}
        </KpiCard>
      </div>

      <!-- Performance + allocation -->
      <div class="grid grid-cols-1 gap-6 lg:grid-cols-3">
        <div class="lg:col-span-2">
          <PortfolioChart data={d.valueTrend} loading={false} />
        </div>

        <!-- Allocation pie chart -->
        <div class="lg:col-span-1">
          <AllocationPie
            data={pieData}
            title="Allocation"
            description="By market value"
            centerLabel="Total"
            centerValue={fmt(d.totalMarketValue)}
            onSelect={(key) => gotoHolding(key)}
          />
        </div>
      </div>
    {/if}

    <HoldingsTable holdings={d.holdings} {currency} />
  {/if}
</div>
