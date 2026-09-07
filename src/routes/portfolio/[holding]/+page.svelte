<script lang="ts">
  import { page } from '$app/state';
  import { resolve } from '$app/paths';
  import { Button } from '$lib/components/ui/button';
  import * as Tabs from '$lib/components/ui/tabs';
  import { ArrowLeft, Download, LoaderCircle, TrendingUp } from '@lucide/svelte';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { appSettings, applyPageSize } from '$lib/stores/settings.svelte';
  import { fetchHoldingDetail, fetchSymbolOverrides, refreshPrices } from '$lib/tauri';
  import type { HoldingTransaction, PriceHistoryEntry } from '$lib/tauri';
  import { toast } from '$lib/components/ui/sonner';
  import { errMsg } from '$lib/utils/error';
  import SymbolOverrideDialog from '$lib/features/portfolio/symbol-override-dialog.svelte';
  import { sessionCurrency, parseLocalDate, formatCurrency } from '$lib/utils/format.svelte';
  import TimeSeriesChart from '$lib/components/shared/time-series-chart.svelte';
  import { ColumnsMenu } from '$lib/components/shared/columns-menu';
  import { useColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageHeaderSkeleton from '$lib/components/shared/page-header-skeleton.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import ChartSkeleton from '$lib/components/shared/chart-skeleton.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import { setBreadcrumb } from '$lib/stores/breadcrumb.svelte';
  import { downloadCsv } from '$lib/utils/csv';
  import { exportPaginatedCsv } from '$lib/utils/export';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useUrlParams, pageParam } from '$lib/hooks/use-url-params.svelte';
  import { holdingLabel, toastPriceResult, range52w } from '$lib/features/portfolio/price-utils';
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

  // Reset to page 1 on holding switch. Declared before the fetch effect so it
  // runs first in the same flush: the fetch sees the reset page immediately
  // instead of firing a doomed request.
  let prevHoldingGuid: string | undefined = undefined;
  $effect(() => {
    const g = holdingGuid;
    if (prevHoldingGuid !== undefined && g !== prevHoldingGuid) {
      if (pg !== 1) pg = 1;
    }
    prevHoldingGuid = g;
  });

  let data = $derived(res.data);
  let loading = $derived(res.loading);
  let error = $derived(res.error);
  let notFound = $derived(error !== null && error.includes('Holding not found'));

  let holding = $derived(data?.holding ?? null);
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

  let refreshingPrices = $state(false);
  let exportingPrices = $state(false);
  let exportingActivity = $state(false);

  /** Activity/Prices tab selection (local; activity pagination owns the URL). */
  let tab = $state('activity');

  const activityCols = useColumnVisibility(ACTIVITY_DEFAULT_COLUMNS);
  const priceCols = useColumnVisibility(PRICE_DEFAULT_COLUMNS);

  async function handleRefreshPrices() {
    if (!dbSession.value || refreshingPrices) return;
    refreshingPrices = true;
    try {
      const result = await refreshPrices();
      toastPriceResult(result);
      // Reload so market values, KPIs and the trend pick up the new prices.
      res.reload();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to refresh prices'));
    } finally {
      refreshingPrices = false;
    }
  }

  async function handleSymbolSave() {
    overridesRes.reload();
    toast.success('Yahoo symbol saved — refreshing prices to apply it');
    await handleRefreshPrices();
  }
  let currency = $derived(sessionCurrency());
  // Price history is stored in its original quote currency, so the chart and
  // its range stats are labeled in that currency rather than the report one.
  // Totals and per-share amounts stay in the report currency (see Option A).
  let priceCurrency = $derived(data?.priceHistory[0]?.currency ?? currency);
  // Quote-currency 52W range, shown on the Price History chart instead of a KPI.
  let priceRange = $derived(range52w(data?.priceHistory));
  let priceHistoryDescription = $derived(
    priceRange
      ? `Per-share price, quoted in ${priceCurrency} · 52W ${formatCurrency(priceRange.low, priceCurrency)} – ${formatCurrency(priceRange.high, priceCurrency)}`
      : `Per-share price, quoted in ${priceCurrency}`
  );

  $effect(() => {
    if (!dbSession.value) return;
    setBreadcrumb([{ name: 'Portfolio', href: resolve('/portfolio') }, { name }]);
  });

  function safeFilename(s: string): string {
    return s.replace(/[^\w.-]+/g, '_');
  }

  function exportPriceHistory() {
    const d = data;
    if (!d || d.priceHistory.length === 0 || exportingPrices) return;
    exportingPrices = true;
    try {
      const headers = ['Mnemonic', 'Currency', 'Date', 'Price'];
      const rows = d.priceHistory.map((e: PriceHistoryEntry) => [
        d.holding.mnemonic,
        e.currency,
        e.date,
        e.price.toFixed(6)
      ]);
      downloadCsv(`${safeFilename(d.holding.mnemonic)}-prices.csv`, headers, rows);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to export prices'));
    } finally {
      exportingPrices = false;
    }
  }

  async function exportTransactions() {
    const d = data;
    const g = holdingGuid;
    if (!d || !g || d.total === 0 || exportingActivity) return;
    exportingActivity = true;
    try {
      await exportPaginatedCsv({
        filename: `${safeFilename(d.holding.mnemonic)}-transactions.csv`,
        headers: [
          'Date',
          'Description',
          'Number',
          'Notes',
          'Account',
          'Shares',
          'Value',
          'Running Shares'
        ],
        fetchPage: async (pageIdx, size) => {
          const r = await fetchHoldingDetail(g, pageIdx, size);
          return { rows: r.transactions, total: r.total };
        },
        mapRow: (t: HoldingTransaction) => [
          t.postDate,
          t.description ?? '',
          t.num ?? '',
          t.notes ?? '',
          t.accountDescription || t.accountName,
          t.sharesDelta.toFixed(6),
          t.valueDelta.toFixed(2),
          t.runningShares.toFixed(6)
        ]
      });
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to export activity'));
    } finally {
      exportingActivity = false;
    }
  }
</script>

<svelte:head>
  <title>{name} — Spendo</title>
  <meta name="description" content="Holding detail: price history, value trend and accounts." />
</svelte:head>

{#if !dbSession.value && !dbBootstrapping.value}
  <div class="flex flex-col gap-6">
    <PageHeader title={name} description="Investment holding detail" />
    <ImportPrompt />
  </div>
{:else if dbBootstrapping.value || (loading && !data)}
  <div class="flex flex-col gap-6">
    <PageHeaderSkeleton />
    <KpiCardRow count={4} />
    <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <ChartSkeleton withFilters />
      <ChartSkeleton withFilters />
    </div>
    <TableSkeleton cols={6} rows={3} />
    <TableSkeleton cols={5} rows={4} />
  </div>
{:else if error && !notFound}
  <ErrorCard message={error} onRetry={() => res.reload()} />
{:else if notFound || !data}
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
  {@const holding = data.holding}

  <div class="flex flex-col gap-6">
    <HoldingDetailHeader
      {holding}
      {name}
      refreshing={refreshingPrices}
      onRefresh={() => void handleRefreshPrices()}
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
    <DimWrapper active={loading} class="grid grid-cols-1 gap-6 xl:grid-cols-2">
      <TimeSeriesChart
        title="Price History"
        description={priceHistoryDescription}
        loading={false}
        emptyMessage="No price history found — set the Yahoo symbol, then refresh prices."
        data={data.priceHistory}
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
        data={data.valueTrend}
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
            <Button
              variant="outline"
              size="sm"
              onclick={exportTransactions}
              disabled={data.total === 0 || exportingActivity}
              title="Export activity as CSV"
            >
              {#if exportingActivity}
                <LoaderCircle aria-hidden="true" class="size-3 animate-spin" />
                Exporting...
              {:else}
                <Download aria-hidden="true" class="size-3" />
                Export
              {/if}
            </Button>
            <ColumnsMenu columns={ACTIVITY_COLUMNS} cols={activityCols} />
          {:else}
            <Button
              variant="outline"
              size="sm"
              onclick={exportPriceHistory}
              disabled={data.priceHistory.length === 0 || exportingPrices}
              title="Export price history as CSV"
            >
              {#if exportingPrices}
                <LoaderCircle aria-hidden="true" class="size-3 animate-spin" />
                Exporting...
              {:else}
                <Download aria-hidden="true" class="size-3" />
                Export
              {/if}
            </Button>
            <ColumnsMenu columns={PRICE_COLUMNS} cols={priceCols} />
          {/if}
        </div>
      </div>
      <Tabs.Content value="activity">
        <HoldingActivityTable
          transactions={data.transactions}
          {currency}
          cols={activityCols}
          {loading}
          bind:page={pg}
          total={data.total}
          totalPages={data.totalPages}
          onPageSizeChange={(size) => applyPageSize(size, (v) => (pg = v))}
        />
      </Tabs.Content>
      <Tabs.Content value="prices">
        <HoldingPricesTable priceHistory={data.priceHistory} {priceCurrency} cols={priceCols} />
      </Tabs.Content>
    </Tabs.Root>
  </div>
{/if}
