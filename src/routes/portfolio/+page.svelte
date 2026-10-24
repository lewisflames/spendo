<script lang="ts">
  import { resolve } from '$app/paths';
  import { tooltip } from '$lib/attachments/tooltip';
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import { HandCoins, TrendingDown, TrendingUp, RefreshCw, Pencil } from '@lucide/svelte';
  import { SvelteMap } from 'svelte/reactivity';
  import { dbBootstrapping, dbSession, activeBookPath } from '$lib/stores/db.svelte';
  import { fetchPortfolioData, fetchAllocationTargets } from '$lib/tauri';
  import {
    signColor,
    sessionCurrency,
    formatSessionCurrency,
    formatSignedCurrency,
    fmtPct,
    formatAllocPct,
    formatDate,
    plural
  } from '$lib/utils/format.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import MissingFxWarning from '$lib/features/reports/missing-fx-warning.svelte';
  import { Button } from '$lib/components/ui/button';
  import PortfolioChart from '$lib/features/portfolio/portfolio-chart.svelte';
  import HoldingsTable from '$lib/features/portfolio/holdings-table.svelte';
  import RebalanceTargetsDialog from '$lib/features/portfolio/rebalance-targets-dialog.svelte';
  import AllocationPie from '$lib/features/accounts/allocation-pie.svelte';
  import { holdingLabel } from '$lib/features/portfolio/price-utils';
  import { createPriceRefresh } from '$lib/features/portfolio/use-price-refresh.svelte';
  import { loadBookSettings } from '$lib/stores/book-settings.svelte';
  import { openHolding } from '$lib/utils/navigation';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';

  const meta = PAGE_META.portfolio;

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: fetchPortfolioData,
    label: 'We couldn’t load your portfolio'
  });

  /** Saved allocation targets; the single source of truth for the rebalance columns. */
  const savedTargets = new SvelteMap<string, number>();
  let targetsOpen = $state(false);

  const targetsRes = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () => {
      // Read the active book path so a book switch refetches targets instead
      // of showing the previous book’s allocation.
      void activeBookPath.value;
      return fetchAllocationTargets();
    },
    label: 'We couldn’t load your targets',
    onSuccess: (rows) => {
      savedTargets.clear();
      for (const row of rows) savedTargets.set(row.commodityGuid, row.targetPct);
    }
  });

  let eligibleHoldings = $derived(
    (res.data?.holdings ?? []).filter(
      (h) => h.accounts.length > 0 && h.price !== null && h.price > 0
    )
  );

  // Saving targets replaces the whole set, so never open the dialog before
  // the saved targets are known (a failed load would otherwise wipe them).
  let canEditTargets = $derived(
    !res.loading && !targetsRes.loading && !targetsRes.error && eligibleHoldings.length > 0
  );

  let currency = $derived(sessionCurrency());

  const priceRefresh = createPriceRefresh(() => res.reload());
  // Book-scoped auto-refresh: once per book per page mount. Keyed by the active
  // book path (switching books keeps this page mounted), not by mount alone.
  // When enabled, fetch if cached prices are missing or older than a day (and
  // there is something to price). Manual refresh stays in the header button.
  let autoRefreshBook = $state<string | null>(null);

  $effect(() => {
    if (!dbSession.value || res.loading || !res.data) return;
    const book = activeBookPath.value;
    if (autoRefreshBook === book) return;
    autoRefreshBook = book;
    if (res.data.holdings.length === 0) return;
    loadBookSettings()
      .then((settings) => {
        if (settings.priceAutoRefresh !== 'on_startup' || priceRefresh.refreshing) return;
        const last = res.data?.lastRefreshed;
        const age = last ? Date.now() - new Date(last).getTime() : Number.POSITIVE_INFINITY;
        if (!Number.isFinite(age) || age > 24 * 3600 * 1000) void priceRefresh.refresh();
      })
      .catch(() => {});
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
    const total = res.data.holdings.reduce((s, h) => s + Math.max(h.marketValue, 0), 0);
    return res.data.holdings.map((h, i) => ({
      key: h.commodityGuid,
      label: holdingLabel(h),
      value: Math.max(h.marketValue, 0),
      color: holdingColor(i),
      sharePct: total > 0 ? (Math.max(h.marketValue, 0) / total) * 100 : null
    }));
  });

  let netWorthSharePct = $derived.by(() => {
    const netWorth = dbSession.value?.summary.netWorth ?? 0;
    if (netWorth <= 0 || !res.data) return null;
    return (res.data.totalMarketValue / netWorth) * 100;
  });

  let headerDescription = $derived.by(() => {
    let desc: string = meta.description;
    if (res.data) {
      desc += ` · ${plural(res.data.holdings.length, 'holding')}`;
    }
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
  <title>{meta.title} · Spendo</title>
  <meta name="description" content={meta.description} />
</svelte:head>

<BookGate title={meta.title} description={meta.description}>
  <div class="page-stack">
    <PageHeader title={meta.title} description={headerDescription}>
      {#if dbSession.value}
        <Button
          variant="outline"
          onclick={() => (targetsOpen = true)}
          disabled={!canEditTargets}
          {@attach tooltip(
            eligibleHoldings.length === 0 ? 'No priced investment holdings to rebalance' : undefined
          )}
        >
          <Pencil aria-hidden="true" class="size-4" />
          Edit targets
        </Button>
        <Button
          variant="outline"
          onclick={() => void priceRefresh.refresh()}
          disabled={res.loading || priceRefresh.refreshing}
        >
          <RefreshCw
            aria-hidden="true"
            class="size-4 {priceRefresh.refreshing ? 'animate-spin' : ''}"
          />
          {priceRefresh.refreshing ? 'Refreshing…' : 'Refresh prices'}
        </Button>
      {/if}
    </PageHeader>

    {#if targetsRes.error}
      <ErrorCard message={targetsRes.error} onRetry={() => targetsRes.reload()} />
    {/if}

    <RebalanceTargetsDialog
      bind:open={targetsOpen}
      holdings={res.data?.holdings ?? []}
      {currency}
      targets={savedTargets}
      onSaved={(next) => {
        savedTargets.clear();
        for (const [commodityGuid, targetPct] of next) savedTargets.set(commodityGuid, targetPct);
      }}
    />

    {#if dbBootstrapping.value || (res.loading && !res.data)}
      <PageSkeleton variant="portfolio" />
      <HoldingsTable loading {currency} holdings={[]} targets={savedTargets} />
    {:else if res.data}
      {@const d = res.data}
      {@const guidByMnemonic = new Map(d.holdings.map((h) => [h.mnemonic, h.commodityGuid]))}
      {#if res.error}
        <ErrorCard message={res.error} onRetry={() => res.reload()} />
      {/if}

      <MissingFxWarning priceErrors={priceRefresh.errors} holdingGuidByMnemonic={guidByMnemonic} />

      <DimWrapper active={res.loading} class="flex flex-col gap-6">
        {#if d.holdings.length > 0}
          <!-- KPI summary cards -->
          <div class="grid grid-cols-1 gap-6 @xl:grid-cols-2 @4xl:grid-cols-4">
            <KpiCard title="Market value" value={fmt(d.totalMarketValue)}>
              {#snippet icon()}
                <TrendingUp aria-hidden="true" class="size-4 text-muted-foreground" />
              {/snippet}
            </KpiCard>
            <KpiCard title="Total return" valueClass={signColor(d.totalReturn)}>
              {#snippet icon()}
                {#if d.totalReturn >= 0}
                  <TrendingUp aria-hidden="true" class="size-4 text-success" />
                {:else}
                  <TrendingDown aria-hidden="true" class="size-4 text-destructive" />
                {/if}
              {/snippet}
              {#snippet value()}
                {formatSignedCurrency(d.totalReturn, currency)}
              {/snippet}
            </KpiCard>
            <KpiCard
              title="All-time dividends"
              value={fmt(d.totalDividends)}
              href={resolve('/portfolio/gains')}
              linkLabel="All-time dividends, open dividends by year"
            >
              {#snippet icon()}
                <HandCoins aria-hidden="true" class="size-4 text-muted-foreground" />
              {/snippet}
            </KpiCard>
            <KpiCard
              title="XIRR"
              valueClass={d.totalXirr !== null ? signColor(d.totalXirr) : 'text-muted-foreground'}
            >
              {#snippet icon()}
                {#if d.totalXirr !== null && d.totalXirr < 0}
                  <TrendingDown aria-hidden="true" class="size-4 text-destructive" />
                {:else}
                  <TrendingUp
                    aria-hidden="true"
                    class="size-4 {d.totalXirr !== null ? 'text-success' : 'text-muted-foreground'}"
                  />
                {/if}
              {/snippet}
              {#snippet value()}
                {fmtPct(d.totalXirr)}
              {/snippet}
            </KpiCard>
          </div>

          <!-- Performance + allocation -->
          <div class="grid grid-cols-1 gap-6 @3xl:grid-cols-3">
            <div class="@3xl:col-span-2">
              <PortfolioChart data={d.valueTrend} loading={false} />
            </div>

            <!-- Allocation pie chart -->
            <div class="@3xl:col-span-1">
              <AllocationPie
                data={pieData}
                title="Allocation"
                description="By market value"
                {currency}
                centerLabel={netWorthSharePct != null ? 'of net worth' : null}
                centerValue={netWorthSharePct != null ? formatAllocPct(netWorthSharePct) : null}
                onSelect={(key) => openHolding(key)}
              />
            </div>
          </div>
        {/if}

        <HoldingsTable holdings={d.holdings} {currency} targets={savedTargets} />
      </DimWrapper>
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {/if}
  </div>
</BookGate>
