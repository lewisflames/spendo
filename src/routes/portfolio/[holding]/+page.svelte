<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Download, Tag } from '@lucide/svelte';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchHoldingDetail, fetchSymbolOverrides } from '$lib/tauri';
  import type { HoldingTransaction, PriceHistoryEntry } from '$lib/tauri';
  import { toast } from '$lib/components/ui/sonner';
  import SymbolOverrideDialog from '$lib/features/portfolio/symbol-override-dialog.svelte';
  import { sessionCurrency, parseLocalDate } from '$lib/utils/format.svelte';
  import TimeSeriesChart from '$lib/components/shared/time-series-chart.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import PageHeaderSkeleton from '$lib/components/shared/page-header-skeleton.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import ChartSkeleton from '$lib/components/shared/chart-skeleton.svelte';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import { setBreadcrumb } from '$lib/stores/breadcrumb.svelte';
  import { downloadCsv } from '$lib/utils/csv';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { holdingLabel } from '$lib/features/portfolio/price-utils';
  import HoldingKpiCards from '$lib/features/portfolio/holding-kpi-cards.svelte';
  import HoldingAccountsTable from '$lib/features/portfolio/holding-accounts-table.svelte';
  import HoldingActivityTable from '$lib/features/portfolio/holding-activity-table.svelte';

  let holdingGuid = $derived(page.params.holding);

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!holdingGuid,
    fetch: () => {
      const g = holdingGuid;
      if (!g) return Promise.reject(new Error('No holding selected'));
      return fetchHoldingDetail(g);
    },
    label: 'Failed to load holding'
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

  function handleSymbolSave() {
    overridesRes.reload();
    toast.success('Yahoo symbol saved — refresh prices in Portfolio to apply it');
  }
  let currency = $derived(sessionCurrency());
  // Price history is stored in its original quote currency, so the chart and
  // its range stats are labeled in that currency rather than the report one.
  let priceCurrency = $derived(data?.priceHistory[0]?.currency ?? currency);

  $effect(() => {
    if (!dbSession.value) return;
    setBreadcrumb([{ name: 'Portfolio', href: resolve('/portfolio') }, { name }]);
  });

  function exportPriceHistory() {
    const d = data;
    if (!d || d.priceHistory.length === 0) return;
    const headers = ['mnemonic', 'currency', 'date', 'price'];
    const rows = d.priceHistory.map((e: PriceHistoryEntry) => [
      d.holding.mnemonic,
      e.currency,
      e.date,
      e.price.toFixed(6)
    ]);
    downloadCsv(`${d.holding.mnemonic}-prices.csv`, headers, rows);
  }

  function exportTransactions() {
    const d = data;
    if (!d || d.transactions.length === 0) return;
    const headers = ['date', 'description', 'account', 'shares', 'value'];
    const rows = d.transactions.map((t: HoldingTransaction) => [
      t.postDate,
      t.description ?? '',
      t.accountName,
      t.sharesDelta.toFixed(6),
      t.valueDelta.toFixed(2)
    ]);
    downloadCsv(`${d.holding.mnemonic}-transactions.csv`, headers, rows);
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
    <KpiCardRow count={6} />
    <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <ChartSkeleton withFilters />
      <ChartSkeleton withFilters />
    </div>
    <TableSkeleton cols={6} rows={3} />
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
      <Tag aria-hidden="true" class="text-muted-foreground" />
    {/snippet}
    {#snippet action()}
      <Button variant="outline" onclick={() => goto(resolve('/portfolio'))}>
        Back to portfolio
      </Button>
    {/snippet}
  </EmptyState>
{:else}
  {@const holding = data.holding}

  <div class="flex flex-col gap-6">
    <PageHeader
      title={name}
      description="{holding.mnemonic} · {holding.namespace}{holding.shares === 0
        ? ' · No open position'
        : ''}"
    >
      <Button
        variant="outline"
        onclick={exportPriceHistory}
        disabled={data.priceHistory.length === 0}
      >
        <Download aria-hidden="true" class="size-3" />
        Export Prices
      </Button>
    </PageHeader>

    <!-- KPI summary cards -->
    <HoldingKpiCards {holding} priceHistory={data.priceHistory} {priceCurrency} />

    <!-- Yahoo symbol override (Yahoo-only pricing; app-local, read-only safe) -->
    <Card.Root>
      <Card.Content class="flex flex-wrap items-center justify-between gap-3 py-4">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium">Yahoo Finance symbol</span>
          <span class="text-xs text-muted-foreground">
            {#if overrideSymbol}
              Override: <span class="font-mono font-medium text-foreground">{overrideSymbol}</span>
            {:else}
              Automatic detection
            {/if}
            {#if holding.price === null}
              · No price yet — check the symbol, then refresh prices in Portfolio.
            {/if}
          </span>
        </div>
        <Button variant="outline" size="sm" onclick={() => (symbolDialogOpen = true)}>
          Edit symbol
        </Button>
      </Card.Content>
    </Card.Root>
    <SymbolOverrideDialog
      bind:open={symbolDialogOpen}
      commodityGuid={holdingGuid ?? ''}
      mnemonic={holding.mnemonic}
      currentSymbol={overrideSymbol}
      onSave={handleSymbolSave}
      onError={(msg) => toast.error(msg)}
    />

    <!-- Charts -->
    <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <TimeSeriesChart
        title="Price History"
        description="Per-share price, quoted in {priceCurrency}"
        {loading}
        emptyMessage="No price history found."
        data={data.priceHistory}
        getX={(d: PriceHistoryEntry) => parseLocalDate(d.date)}
        yField="price"
        series={[{ key: 'price', label: 'Price', color: 'var(--color-chart-1)' }]}
        currency={priceCurrency}
      />
      <TimeSeriesChart
        title="Value Trend"
        description="Market value vs cost basis over time"
        {loading}
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
    </div>

    <!-- Accounts holding this security -->
    <HoldingAccountsTable {holding} {currency} />

    <!-- Buy/Sell activity -->
    <HoldingActivityTable
      transactions={data.transactions}
      {currency}
      onExport={exportTransactions}
    />
  </div>
{/if}
