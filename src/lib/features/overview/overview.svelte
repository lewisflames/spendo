<script lang="ts">
  import { resolve } from '$app/paths';
  import {
    ArrowRight,
    CalendarClock,
    CircleCheck,
    Scale,
    TrendingDown,
    TrendingUp,
    Wallet
  } from '@lucide/svelte';
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import EmptyBookPrompt from '$lib/components/shared/empty-book-prompt.svelte';
  import PnlTrend from '$lib/features/pnl/pnl-trend.svelte';
  import TransactionTable from '$lib/features/transactions/transaction-table.svelte';
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import { scheduleStatus } from '$lib/features/schedules/schedule-utils';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { navBadges } from '$lib/stores/nav.svelte';
  import { fetchPnlReport, fetchPnlTrend, fetchSchedules, fetchTransactions } from '$lib/tauri';
  import { computePrevRange, todayStr } from '$lib/utils/date-utils.svelte';
  import { modKeyLabel } from '$lib/utils/platform';
  import {
    clampZero,
    formatDate,
    formatSessionCurrency as fmt,
    plural,
    sessionCurrency,
    signColor
  } from '$lib/utils/format.svelte';

  const meta = PAGE_META.overview;
  const modKey = modKeyLabel();
  const RECENT_COUNT = 8;

  let today = todayStr();
  let monthStart = `${today.slice(0, 7)}-01`;
  // Twelve whole months up to and including the current one.
  let trendStart = $derived.by(() => {
    const [y, m] = today.split('-').map(Number);
    const d = new Date(y, m - 12, 1);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-01`;
  });

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

  const recent = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () => {
      void dbSession.value?.summary.totalTransactions;
      return fetchTransactions(1, RECENT_COUNT, {}, 'date', 'desc');
    },
    label: 'We couldn’t load recent transactions'
  });

  const schedules = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: () => {
      void navBadges.value.dueSchedules;
      return fetchSchedules();
    },
    label: 'We couldn’t load your schedules'
  });

  let dueSchedules = $derived(
    (schedules.data ?? [])
      .filter((s) => scheduleStatus(s, today) === 'overdue')
      .sort((a, b) => (a.nextDueDate ?? '').localeCompare(b.nextDueDate ?? ''))
      .slice(0, 5)
  );

  let monthLoading = $derived(month.loading && !month.data);
</script>

<div class="page-stack">
  <PageHeader title={meta.title} description={meta.description} />

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
      >
        {#snippet icon()}
          <Wallet aria-hidden="true" class="size-4 text-muted-foreground" />
        {/snippet}
      </KpiCard>
      <KpiCard
        title="Income this month"
        value={month.data ? fmt(clampZero(month.data.totalIncome)) : undefined}
        loading={monthLoading}
      >
        {#snippet icon()}
          <TrendingUp aria-hidden="true" class="size-4 text-muted-foreground" />
        {/snippet}
      </KpiCard>
      <KpiCard
        title="Spending this month"
        value={month.data ? fmt(clampZero(month.data.totalExpenses)) : undefined}
        loading={monthLoading}
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
      >
        {#snippet icon()}
          <Scale aria-hidden="true" class="size-4 text-muted-foreground" />
        {/snippet}
      </KpiCard>
    </div>

    <div class="grid grid-cols-1 items-start gap-6 @3xl:grid-cols-3">
      <div class="@3xl:col-span-2">
        {#if trend.error && !trend.data}
          <ErrorCard message={trend.error} onRetry={() => trend.reload()} />
        {:else}
          <PnlTrend
            data={trend.data ?? []}
            loading={trend.loading && !trend.data}
            description="The last 12 months"
          />
        {/if}
      </div>

      <Card.Root>
        <IconCardHeader title="Needs attention">
          {#snippet icon()}
            <CircleCheck aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </IconCardHeader>
        <Card.Content class="flex flex-col gap-4">
          <a
            href={resolve('/reconcile')}
            class="flex items-center justify-between gap-3 rounded-lg border px-3 py-2.5 text-sm transition-colors hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          >
            <span class="flex items-center gap-2">
              <Scale aria-hidden="true" class="size-4 text-muted-foreground" />
              Uncleared transactions
            </span>
            <span class="font-medium tabular-nums">{navBadges.value.unclearedTransactions}</span>
          </a>

          <div class="flex flex-col gap-2">
            <div class="flex items-center justify-between gap-2 text-sm">
              <span class="flex items-center gap-2">
                <CalendarClock aria-hidden="true" class="size-4 text-muted-foreground" />
                Due schedules
              </span>
              <span class="font-medium tabular-nums">{navBadges.value.dueSchedules}</span>
            </div>
            {#if dueSchedules.length > 0}
              <ul class="flex flex-col divide-y rounded-lg border">
                {#each dueSchedules as schedule (schedule.guid)}
                  <li class="flex items-center justify-between gap-3 px-3 py-2 text-sm">
                    <span class="min-w-0 truncate">{schedule.name}</span>
                    <span class="shrink-0 text-xs text-muted-foreground">
                      {schedule.nextDueDate ? formatDate(schedule.nextDueDate) : ''}
                    </span>
                  </li>
                {/each}
              </ul>
            {:else if schedules.data}
              <p class="text-sm text-muted-foreground">Nothing due. You’re all caught up.</p>
            {/if}
          </div>

          <Button variant="outline" href={resolve('/scheduled')} class="self-start">
            Scheduled
            <ArrowRight aria-hidden="true" class="size-4" />
          </Button>
        </Card.Content>
      </Card.Root>
    </div>

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
