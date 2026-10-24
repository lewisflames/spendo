<script lang="ts">
  import { resolve } from '$app/paths';
  import {
    ArrowRight,
    CalendarClock,
    Scale,
    TrendingDown,
    TrendingUp,
    Wallet
  } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import EmptyBookPrompt from '$lib/components/shared/empty-book-prompt.svelte';
  import PnlTrend from '$lib/features/pnl/pnl-trend.svelte';
  import NetWorthChart from '$lib/features/overview/net-worth-chart.svelte';
  import TransactionTable from '$lib/features/transactions/transaction-table.svelte';
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { navBadges } from '$lib/stores/nav.svelte';
  import { fetchNetWorthTrend, fetchPnlReport, fetchPnlTrend, fetchTransactions } from '$lib/tauri';
  import { computePrevRange, todayStr } from '$lib/utils/date-utils.svelte';
  import { modKeyLabel } from '$lib/utils/platform';
  import { tooltip } from '$lib/attachments/tooltip';
  import {
    clampZero,
    formatNumber,
    formatSessionCurrency as fmt,
    plural,
    sessionCurrency,
    signColor
  } from '$lib/utils/format.svelte';

  const meta = PAGE_META.overview;
  const modKey = modKeyLabel();
  const RECENT_COUNT = 8;
  const PILL_CLASS =
    'flex h-8 items-center gap-1.5 rounded-full border bg-card px-3 text-sm transition-colors hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none';

  let today = todayStr();
  let monthStart = `${today.slice(0, 7)}-01`;
  // Twelve whole months up to and including the current one.
  let trendStart = $derived.by(() => {
    const [y, m] = today.split('-').map(Number);
    const d = new Date(y, m - 12, 1);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-01`;
  });

  // The month cards open their report for the same stretch (query-only suffix).
  const monthQuery = `?from=${monthStart}&to=${today}`;
  const pnlMonthHref = resolve('/reports/pnl') + monthQuery;
  const spendingMonthHref = resolve('/reports/spending') + monthQuery;

  let currency = $derived(sessionCurrency());
  let summary = $derived(dbSession.value?.summary ?? null);

  const month = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () => {
      void dbSession.value?.summary.totalTransactions;
      const { prevFrom, prevTo } = computePrevRange(monthStart, today);
      return fetchPnlReport(monthStart, today, prevFrom, prevTo);
    },
    label: 'We couldn’t load this month’s totals'
  });

  const trend = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () => {
      void dbSession.value?.summary.totalTransactions;
      return fetchPnlTrend(trendStart, today);
    },
    label: 'We couldn’t load that chart'
  });

  const netWorth = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () => {
      // Also track the KPI value so a price refresh redraws the line with it.
      void dbSession.value?.summary.totalTransactions;
      void dbSession.value?.summary.netWorth;
      return fetchNetWorthTrend();
    },
    label: 'We couldn’t load that chart'
  });

  const recent = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () => {
      void dbSession.value?.summary.totalTransactions;
      return fetchTransactions(1, RECENT_COUNT, {}, 'date', 'desc');
    },
    label: 'We couldn’t load recent transactions'
  });

  let uncleared = $derived(navBadges.value.unclearedTransactions);
  let dueCount = $derived(navBadges.value.dueSchedules);

  let monthLoading = $derived(month.loading && !month.data);
</script>

{#snippet pillContent(Icon: typeof Scale, label: string)}
  <Icon aria-hidden="true" class="size-3.5 text-muted-foreground" />
  <span class="font-medium tabular-nums">{label}</span>
  <ArrowRight aria-hidden="true" class="size-3.5 text-muted-foreground" />
{/snippet}

<div class="page-stack">
  <!-- Needs-attention pills sit in the header's action slot and only appear
       when there is something to do. -->
  <PageHeader title={meta.title} description={meta.description}>
    {#if uncleared > 0}
      {@const hint = `Reconcile ${plural(uncleared, 'uncleared transaction')}`}
      <a href={resolve('/reconcile')} class={PILL_CLASS} aria-label={hint} {@attach tooltip(hint)}>
        {@render pillContent(Scale, `${formatNumber(uncleared)} uncleared`)}
      </a>
    {/if}
    {#if dueCount > 0}
      {@const hint = `Review ${plural(dueCount, 'due schedule')}`}
      <a href={resolve('/scheduled')} class={PILL_CLASS} aria-label={hint} {@attach tooltip(hint)}>
        {@render pillContent(CalendarClock, `${formatNumber(dueCount)} due`)}
      </a>
    {/if}
  </PageHeader>

  {#if summary && summary.totalAccounts === 0}
    <EmptyBookPrompt />
  {:else}
    {#if month.error}
      <ErrorCard message={month.error} onRetry={() => month.reload()} />
    {/if}

    <div class="grid grid-cols-1 gap-6 @xl:grid-cols-2 @4xl:grid-cols-4">
      <KpiCard
        title="Net worth"
        value={summary ? fmt(clampZero(summary.netWorth)) : undefined}
        valueClass={summary ? signColor(summary.netWorth) : ''}
        loading={!summary}
        href={resolve('/reports/balance-sheet')}
        linkLabel="Net worth, open the balance sheet"
      >
        {#snippet icon()}
          <Wallet aria-hidden="true" class="size-4 text-muted-foreground" />
        {/snippet}
      </KpiCard>
      <KpiCard
        title="Income this month"
        value={month.data ? fmt(clampZero(month.data.totalIncome)) : undefined}
        loading={monthLoading}
        href={pnlMonthHref}
        linkLabel="Income this month, open profit & loss"
      >
        {#snippet icon()}
          <TrendingUp aria-hidden="true" class="size-4 text-muted-foreground" />
        {/snippet}
      </KpiCard>
      <KpiCard
        title="Spending this month"
        value={month.data ? fmt(clampZero(month.data.totalExpenses)) : undefined}
        loading={monthLoading}
        href={spendingMonthHref}
        linkLabel="Spending this month, open the spending report"
      >
        {#snippet icon()}
          <TrendingDown aria-hidden="true" class="size-4 text-muted-foreground" />
        {/snippet}
      </KpiCard>
      <KpiCard
        title="Net this month"
        value={month.data ? fmt(clampZero(month.data.netIncome)) : undefined}
        valueClass={month.data ? signColor(month.data.netIncome) : ''}
        loading={monthLoading}
        href={pnlMonthHref}
        linkLabel="Net this month, open profit & loss"
      >
        {#snippet icon()}
          <Scale aria-hidden="true" class="size-4 text-muted-foreground" />
        {/snippet}
      </KpiCard>
    </div>

    {#if netWorth.error && !netWorth.data}
      <ErrorCard message={netWorth.error} onRetry={() => netWorth.reload()} />
    {:else}
      <NetWorthChart data={netWorth.data ?? []} loading={netWorth.loading && !netWorth.data} />
    {/if}

    {#if trend.error && !trend.data}
      <ErrorCard message={trend.error} onRetry={() => trend.reload()} />
    {:else}
      <PnlTrend
        data={trend.data ?? []}
        loading={trend.loading && !trend.data}
        description="The last 12 months"
      />
    {/if}

    <section class="flex flex-col gap-3" aria-labelledby="recent-heading">
      <div class="flex items-center justify-between gap-2">
        <h2 id="recent-heading" class="text-base font-semibold">Recent transactions</h2>
        <Button variant="ghost" size="sm" href={resolve('/transactions')}>
          View all
          <ArrowRight aria-hidden="true" class="size-3.5" />
        </Button>
      </div>
      {#if recent.loading && !recent.data}
        <TableSkeleton cols={3} rows={RECENT_COUNT} />
      {:else if recent.error && !recent.data}
        <ErrorCard message={recent.error} onRetry={() => recent.reload()} />
      {:else if recent.data && recent.data.transactions.length > 0}
        <TransactionTable
          transactions={recent.data.transactions}
          {currency}
          visibleColumns={['date', 'amount']}
        />
        <p class="text-xs text-muted-foreground">
          Showing the latest {Math.min(RECENT_COUNT, recent.data.total)} of {plural(
            recent.data.total,
            'transaction'
          )}
        </p>
      {:else if recent.data}
        <p class="text-sm text-muted-foreground">
          No transactions yet. Add one with New transaction in the sidebar, or press {modKey}+K.
        </p>
      {/if}
    </section>
  {/if}
</div>
