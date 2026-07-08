<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Chart from '$lib/components/ui/chart';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { PieChart } from 'layerchart';
  import { Briefcase, TrendingUp, TrendingDown, RefreshCw } from '@lucide/svelte';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
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
  import ChartTooltipRow from '$lib/components/ui/chart/chart-tooltip-row.svelte';
  import { toast } from '$lib/components/ui/sonner';
  import PortfolioChart from '$lib/features/portfolio/portfolio-chart.svelte';
  import HoldingsTable from '$lib/features/portfolio/holdings-table.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';

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
        toast.warning(
          `Prices refreshed with ${result.errors.length} error${result.errors.length === 1 ? '' : 's'}`
        );
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

  let chartConfig: Chart.ChartConfig = $derived.by(() => {
    const cfg: Chart.ChartConfig = {};
    if (res.data) {
      for (const h of res.data.holdings) {
        cfg[h.commodityGuid] = { label: h.fullName ?? h.mnemonic };
      }
    }
    return cfg;
  });

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
    return res.data.holdings.map((h, i) => ({
      key: h.commodityGuid,
      label: h.fullName ?? h.mnemonic,
      value: Math.max(h.marketValue, 0),
      color: holdingColor(i)
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
        <Card.Root class="lg:col-span-1">
          <Card.Header>
            <Card.Title>Allocation</Card.Title>
            <Card.Description>By market value</Card.Description>
          </Card.Header>
          <Card.Content>
            {#if pieData.length > 0}
              <div class="relative">
                <Chart.Container config={chartConfig} class="aspect-auto h-72 w-full">
                  <PieChart
                    data={pieData}
                    key="key"
                    label="label"
                    value="value"
                    innerRadius={0.6}
                    cornerRadius={4}
                    padAngle={0.035}
                    c="color"
                    onArcClick={(_, detail) => {
                      goto(
                        resolve('/portfolio/[holding]', {
                          holding: detail.data.key as string
                        })
                      );
                    }}
                  >
                    {#snippet tooltip()}
                      <Chart.Tooltip hideLabel>
                        {#snippet formatter({ value, item })}
                          {@const holding = res.data?.holdings.find(
                            (h) => h.commodityGuid === item.key
                          )}
                          <ChartTooltipRow
                            color={item.color as string}
                            label={holding?.fullName ?? holding?.mnemonic ?? String(item.key)}
                            value={fmt(value as number)}
                            valueSuffix={holding
                              ? `${holding.allocationPct.toFixed(1)}%`
                              : undefined}
                          />
                        {/snippet}
                      </Chart.Tooltip>
                    {/snippet}
                  </PieChart>
                </Chart.Container>
                <div class="pointer-events-none absolute inset-0 flex items-center justify-center">
                  <div class="text-center">
                    <div class="text-xs text-muted-foreground">Total</div>
                    <div class="text-sm font-medium tabular-nums">{fmt(d.totalMarketValue)}</div>
                  </div>
                </div>
              </div>
            {/if}
          </Card.Content>
        </Card.Root>
      </div>
    {/if}

    <HoldingsTable holdings={d.holdings} {currency} />
  {/if}
</div>
