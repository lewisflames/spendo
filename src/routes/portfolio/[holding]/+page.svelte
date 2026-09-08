<script lang="ts">
  import { page } from '$app/state';
  import { resolve } from '$app/paths';
  import { Button } from '$lib/components/ui/button';
  import * as Tabs from '$lib/components/ui/tabs';
  import { ArrowLeft, TrendingUp } from '@lucide/svelte';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { appSettings, applyPageSize } from '$lib/stores/settings.svelte';
  import { fetchHoldingDetail, fetchSymbolOverrides } from '$lib/tauri';
  import type { PriceHistoryEntry } from '$lib/tauri';
  import { toast } from '$lib/components/ui/sonner';
  import SymbolOverrideDialog from '$lib/features/portfolio/symbol-override-dialog.svelte';
  import { sessionCurrency, parseLocalDate, formatCurrency } from '$lib/utils/format.svelte';
  import TimeSeriesChart from '$lib/components/shared/time-series-chart.svelte';
  import { ColumnsMenu } from '$lib/components/shared/columns-menu';
  import { useColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { setBreadcrumb } from '$lib/stores/breadcrumb.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useResetPageOnChange } from '$lib/hooks/use-page-reset.svelte';
  import { useUrlParams, pageParam } from '$lib/hooks/use-url-params.svelte';
  import { holdingLabel, range52w } from '$lib/features/portfolio/price-utils';
  import { createPriceRefresh } from '$lib/features/portfolio/use-price-refresh.svelte';
  import HoldingDetailHeader from '$lib/features/portfolio/holding-detail-header.svelte';
  import HoldingKpiCards from '$lib/features/portfolio/holding-kpi-cards.svelte';
  import HoldingAccountsTable from '$lib/features/portfolio/holding-accounts-table.svelte';
  import HoldingActivityTable, {
    ACTIVITY_COLUMNS,
    ACTIVITY_DEFAULT_COLUMNS
  } from '$lib/features/portfolio/holding-activity-table.svelte';
  import HoldingPricesTable, {
    PRICE_COLUMNS,
    PRICE_DEFAULT_COLUMNS
  } from '$lib/features/portfolio/holding-prices-table.svelte';

  let holdingGuid = $derived(page.params.holding);

  let pageSize = $derived(appSettings.value.pageSize);
  // `pg`, not `page`: the $app/state `page` import above owns that name.
  let pg = $state(1);

  useUrlParams([
    pageParam(
      'page',
      () => pg,
      (p) => (pg = p)
    )
  ]);

  // Reset to page 1 on holding switch. Runs before the fetch effect in the
  // same flush, so the fetch sees the reset page immediately.
  useResetPageOnChange(
    () => holdingGuid,
    () => {
      pg = 1;
    }
  );

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!holdingGuid,
    fetch: () => {
      const g = holdingGuid;
      if (!g) return Promise.reject(new Error('No holding selected'));
      return fetchHoldingDetail(g, pg, pageSize);
    },
    label: 'Failed to load holding',
    onSuccess: (result) => {
      // The backend clamps out-of-range pages; snap back to the valid page
      // (possible after shrinking pageSize).
      if (pg !== result.page) pg = result.page;
    }
  });

  let notFound = $derived(res.error !== null && res.error.includes('Holding not found'));

  let holding = $derived(res.data?.holding ?? null);
  let name = $derived(holding ? holdingLabel(holding) : 'Holding');

  const overridesRes = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: fetchSymbolOverrides,
    label: 'Failed to load Yahoo symbol overrides'
  });

  let overrideSymbol = $derived(
    overridesRes.data?.find((o) => o.commodityGuid === holdingGuid)?.symbol ?? null
  );
  let symbolDialogOpen = $state(false);

  const priceRefresh = createPriceRefresh(() => res.reload());

  /** Activity/Prices tab selection (local; activity pagination owns the URL). */
  let tab = $state('activity');

  const activityCols = useColumnVisibility(ACTIVITY_DEFAULT_COLUMNS);
  const priceCols = useColumnVisibility(PRICE_DEFAULT_COLUMNS);

  async function handleSymbolSave() {
    overridesRes.reload();
    toast.success('Yahoo symbol saved — refreshing prices to apply it');
    await priceRefresh.refresh();
  }
  let currency = $derived(sessionCurrency());
  // Price history is stored in its original quote currency, so the chart and
  // its range stats are labeled in that currency rather than the report one.
  // Totals and per-share amounts stay in the report currency (see Option A).
  let priceCurrency = $derived(res.data?.priceHistory[0]?.currency ?? currency);
  // Quote-currency 52W range, shown on the Price History chart instead of a KPI.
  let priceRange = $derived(range52w(res.data?.priceHistory));
  let priceHistoryDescription = $derived(
    priceRange
      ? `Per-share price, quoted in ${priceCurrency} · 52W ${formatCurrency(priceRange.low, priceCurrency)} – ${formatCurrency(priceRange.high, priceCurrency)}`
      : `Per-share price, quoted in ${priceCurrency}`
  );

  $effect(() => {
    if (!dbSession.value) return;
    // Track navigation explicitly so the crumb refreshes even when the
    // payload is unchanged; title-bar falls back to the route title otherwise.
    void page.url.pathname;
    setBreadcrumb([{ name: 'Portfolio', href: resolve('/portfolio') }, { name }]);
  });
</script>

<svelte:head>
  <title>{name} — Spendo</title>
  <meta name="description" content="Holding detail: price history, value trend and accounts." />
</svelte:head>

<BookGate title={name} description="Investment holding detail">
  {#if dbBootstrapping.value || (res.loading && !res.data)}
    <div class="flex flex-col gap-6">
      <PageSkeleton variant="holding" />
    </div>
  {:else if res.error && !notFound}
    <ErrorCard message={res.error} onRetry={() => res.reload()} />
  {:else if notFound || !res.data}
    <EmptyState
      card
      title="Holding not found"
      description="This security does not exist in your book."
    >
      {#snippet icon()}
        <TrendingUp aria-hidden="true" class="text-muted-foreground" />
      {/snippet}
      {#snippet action()}
        <Button variant="outline" href={resolve('/portfolio')}>
          <ArrowLeft aria-hidden="true" class="size-3" />
          Back to portfolio
        </Button>
      {/snippet}
    </EmptyState>
  {:else}
    {@const holding = res.data.holding}

    <div class="flex flex-col gap-6">
      <HoldingDetailHeader
        {holding}
        {name}
        refreshing={priceRefresh.refreshing}
        onRefresh={() => void priceRefresh.refresh()}
        onEditSymbol={() => (symbolDialogOpen = true)}
      />

      <!-- KPI summary cards -->
      <HoldingKpiCards {holding} />

      <SymbolOverrideDialog
        bind:open={symbolDialogOpen}
        commodityGuid={holdingGuid ?? ''}
        mnemonic={holding.mnemonic}
        currentSymbol={overrideSymbol}
        onSave={() => void handleSymbolSave()}
        onError={(msg) => toast.error(msg)}
      />

      <!-- Charts (previous data stays visible, dimmed, while refetching) -->
      <DimWrapper active={res.loading} class="grid grid-cols-1 gap-6 xl:grid-cols-2">
        <TimeSeriesChart
          title="Price History"
          description={priceHistoryDescription}
          loading={false}
          emptyMessage="No price history found — set the Yahoo symbol, then refresh prices."
          data={res.data.priceHistory}
          getX={(d: PriceHistoryEntry) => parseLocalDate(d.date)}
          yField="price"
          series={[{ key: 'price', label: 'Price', color: 'var(--color-chart-1)' }]}
          currency={priceCurrency}
        />
        <TimeSeriesChart
          title="Value Trend"
          description="Market value vs cost basis over time"
          loading={false}
          emptyMessage="No performance data available."
          data={res.data.valueTrend}
          getX={(d: { date: string }) => parseLocalDate(d.date)}
          series={[
            { key: 'marketValue', label: 'Market Value', color: 'var(--color-success)' },
            { key: 'costBasis', label: 'Cost Basis', color: 'var(--color-warning)' }
          ]}
          {currency}
          tooltipTotals={[
            {
              label: 'Gain / Loss',
              compute: (d) => d.marketValue - d.costBasis
            }
          ]}
        />
      </DimWrapper>

      <!-- Accounts holding this security -->
      <HoldingAccountsTable {holding} {currency} />

      <!-- Activity / prices (tabbed to overlap; one toolbar row for both) -->
      <Tabs.Root bind:value={tab} class="flex flex-col gap-3">
        <div class="flex items-center justify-between gap-2">
          <Tabs.List aria-label="Holding data">
            <Tabs.Trigger value="activity">Activity</Tabs.Trigger>
            <Tabs.Trigger value="prices">Prices</Tabs.Trigger>
          </Tabs.List>
          <div class="flex items-center gap-2">
            {#if tab === 'activity'}
              <ColumnsMenu columns={ACTIVITY_COLUMNS} cols={activityCols} />
            {:else}
              <ColumnsMenu columns={PRICE_COLUMNS} cols={priceCols} />
            {/if}
          </div>
        </div>
        <Tabs.Content value="activity">
          <HoldingActivityTable
            transactions={res.data.transactions}
            {currency}
            cols={activityCols}
            loading={res.loading}
            bind:page={pg}
            total={res.data.total}
            totalPages={res.data.totalPages}
            onPageSizeChange={(size) => applyPageSize(size, (v) => (pg = v))}
          />
        </Tabs.Content>
        <Tabs.Content value="prices">
          <HoldingPricesTable
            priceHistory={res.data.priceHistory}
            {priceCurrency}
            cols={priceCols}
          />
        </Tabs.Content>
      </Tabs.Root>
    </div>
  {/if}
</BookGate>
