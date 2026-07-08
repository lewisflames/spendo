<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import * as Card from '$lib/components/ui/card';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import { Button } from '$lib/components/ui/button';
  import { Briefcase, TrendingUp, TrendingDown, Download, Tag, LineChart } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchHoldingDetail } from '$lib/tauri';
  import type { HoldingTransaction, PortfolioHolding, PriceHistoryEntry } from '$lib/tauri';
  import {
    signColor,
    fmtPct,
    formatSignedCurrency,
    formatSessionCurrency as fmt,
    sessionCurrency,
    formatDate
  } from '$lib/utils/format.svelte';
  import TimeSeriesChart from '$lib/components/shared/time-series-chart.svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import KpiCardRow from '$lib/components/shared/kpi-card-row.svelte';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import { setBreadcrumb } from '$lib/stores/breadcrumb.svelte';
  import { downloadCsv } from '$lib/utils/csv';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';

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
  let name = $derived(holding ? (holding.fullName ?? holding.mnemonic) : 'Holding');
  let currency = $derived(sessionCurrency());
  // Price history is stored in its original quote currency, so the chart and
  // its range stats are labeled in that currency rather than the report one.
  let priceCurrency = $derived(data?.priceHistory[0]?.currency ?? currency);

  $effect(() => {
    if (!dbSession.value) return;
    setBreadcrumb([{ name: 'Portfolio', href: resolve('/portfolio') }, { name }]);
  });

  const STALE_PRICE_DAYS = 7;

  function priceStaleDays(priceDate: string): number {
    return Math.floor((Date.now() - new Date(priceDate).getTime()) / 86_400_000);
  }

  function dayChangePct(h: PortfolioHolding): number | null {
    if (h.price === null || h.prevPrice === null || h.prevPrice === 0) return null;
    return ((h.price - h.prevPrice) / h.prevPrice) * 100;
  }

  let range52w = $derived.by(() => {
    const history = data?.priceHistory;
    if (!history || history.length === 0) return null;
    const cutoff = Date.now() - 365 * 24 * 60 * 60 * 1000;
    const window = history.filter((e) => new Date(e.date).getTime() >= cutoff);
    const source = window.length > 0 ? window : history;
    const prices = source.map((e) => e.price);
    return { high: Math.max(...prices), low: Math.min(...prices) };
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

{#if !dbSession.value}
  <div class="flex flex-col gap-6">
    <PageHeader title={name} description="Investment holding detail" />
    <ImportPrompt />
  </div>
{:else if loading && !data}
  <div class="flex flex-col gap-6">
    <KpiCardRow />
    <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <Card.Root>
        <Card.Content>
          <Skeleton class="h-72 w-full" />
        </Card.Content>
      </Card.Root>
      <Card.Root>
        <Card.Content>
          <Skeleton class="h-72 w-full" />
        </Card.Content>
      </Card.Root>
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
  {@const h = data.holding}

  <div class="flex flex-col gap-6">
    <PageHeader
      title={name}
      description="{h.mnemonic} · {h.namespace}{h.shares === 0 ? ' · No open position' : ''}"
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
          {#if range52w}
            <span class="tabular-nums">
              {range52w.low.toFixed(2)} – {range52w.high.toFixed(2)}
              {priceCurrency}
            </span>
          {:else}
            —
          {/if}
        {/snippet}
      </KpiCard>
      <KpiCard
        title="Allocation"
        value={h.allocationPct > 0 ? `${h.allocationPct.toFixed(1)}%` : '—'}
      >
        {#snippet icon()}
          <Briefcase aria-hidden="true" class="size-4 text-muted-foreground" />
        {/snippet}
      </KpiCard>
    </div>

    <!-- Charts -->
    <div class="grid grid-cols-1 gap-6 lg:grid-cols-2">
      <TimeSeriesChart
        title="Price History"
        description="Per-share price, quoted in {priceCurrency}"
        {loading}
        emptyMessage="No price history found."
        data={data.priceHistory}
        getX={(d: PriceHistoryEntry) => new Date(d.date)}
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
        getX={(d: { date: string }) => new Date(d.date)}
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
    {#if h.accounts.length > 0}
      <TableShell>
        <Table.Header>
          <Table.Row class="bg-muted/75">
            <Table.Head class="text-muted-foreground">Account</Table.Head>
            <Table.Head class="text-right text-muted-foreground">Shares</Table.Head>
            <Table.Head class="text-right text-muted-foreground">Avg Cost</Table.Head>
            <Table.Head class="text-right text-muted-foreground">Cost Basis</Table.Head>
            <Table.Head class="text-right text-muted-foreground">Market Value</Table.Head>
            <Table.Head class="text-right text-muted-foreground">Gain / Loss</Table.Head>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {#each h.accounts as acct (acct.accountGuid)}
            {@const unitPrice = h.shares > 0 && h.price !== null ? h.price : 0}
            {@const gainPerShare = h.shares > 0 ? h.gainLoss / h.shares : 0}
            {@const acctGain = acct.shares * gainPerShare}
            <Table.Row>
              <Table.Cell class="py-2.5">
                <a
                  href={resolve('/accounts/[guid]', { guid: acct.accountGuid })}
                  class="font-medium"
                >
                  {acct.accountName}
                </a>
              </Table.Cell>
              <Table.Cell class="py-2.5 text-right tabular-nums">
                {acct.shares.toFixed(2)}
              </Table.Cell>
              <Table.Cell class="py-2.5 text-right text-muted-foreground tabular-nums">
                {acct.shares > 0 ? fmt(acct.costBasis / acct.shares) : '\u2014'}
              </Table.Cell>
              <Table.Cell class="py-2.5 text-right text-muted-foreground tabular-nums">
                {fmt(acct.costBasis)}
              </Table.Cell>
              <Table.Cell class="py-2.5 text-right font-medium tabular-nums">
                {fmt(acct.shares * unitPrice)}
              </Table.Cell>
              <Table.Cell class="py-2.5 text-right tabular-nums {signColor(acctGain)}">
                {formatSignedCurrency(acctGain, currency)}
              </Table.Cell>
            </Table.Row>
          {/each}
        </Table.Body>
      </TableShell>
    {:else}
      <EmptyState
        card
        title="No accounts hold this security"
        description="The price history is tracked without an open position in your book."
      >
        {#snippet icon()}
          <Briefcase aria-hidden="true" class="text-muted-foreground" />
        {/snippet}
      </EmptyState>
    {/if}

    <!-- Buy/Sell activity -->
    {#if data.transactions.length > 0}
      <div>
        <div class="mb-2 flex items-center justify-between">
          <h2 class="text-sm font-medium text-foreground">Buy / Sell Activity</h2>
          <Button variant="ghost" size="sm" onclick={exportTransactions}>
            <Download aria-hidden="true" class="size-3" />
            Export
          </Button>
        </div>
        <TableShell>
          <Table.Header>
            <Table.Row class="bg-muted/75">
              <Table.Head class="text-muted-foreground">Date</Table.Head>
              <Table.Head class="text-muted-foreground">Description</Table.Head>
              <Table.Head class="text-muted-foreground">Account</Table.Head>
              <Table.Head class="text-right text-muted-foreground">Shares</Table.Head>
              <Table.Head class="text-right text-muted-foreground">Value</Table.Head>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {#each data.transactions as t (t.guid)}
              <Table.Row>
                <Table.Cell class="py-2 text-muted-foreground tabular-nums">
                  {formatDate(t.postDate)}
                </Table.Cell>
                <Table.Cell class="max-w-64 truncate py-2">{t.description ?? '—'}</Table.Cell>
                <Table.Cell class="py-2 text-xs text-muted-foreground">{t.accountName}</Table.Cell>
                <Table.Cell class="py-2 text-right tabular-nums {signColor(t.sharesDelta)}">
                  {t.sharesDelta >= 0 ? '+' : ''}{t.sharesDelta.toFixed(2)}
                </Table.Cell>
                <Table.Cell class="py-2 text-right tabular-nums {signColor(t.sharesDelta)}">
                  {formatSignedCurrency(t.valueDelta, currency)}
                </Table.Cell>
              </Table.Row>
            {/each}
          </Table.Body>
        </TableShell>
      </div>
    {/if}
  </div>
{/if}
