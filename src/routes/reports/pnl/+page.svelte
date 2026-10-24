<script lang="ts">
  import { PAGE_META } from '$lib/features/nav/page-meta';
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchPnlReport, fetchPnlTrend } from '$lib/tauri';
  import {
    signColor,
    formatSessionCurrency,
    formatSignedSessionCurrency,
    formatAllocPct,
    EMPTY_DASH
  } from '$lib/utils/format.svelte';
  import { ArrowLeftRight, History, PiggyBank, TrendingDown, TrendingUp } from '@lucide/svelte';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import PageSkeleton from '$lib/components/shared/page-skeleton.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import BookGate from '$lib/components/shared/book-gate.svelte';
  import PnlTreetable from '$lib/features/pnl/pnl-treetable.svelte';
  import PnlTrend from '$lib/features/pnl/pnl-trend.svelte';
  import MissingFxWarning from '$lib/features/reports/missing-fx-warning.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useReportDateRange } from '$lib/hooks/use-report-date-range.svelte';
  import { useBookDefaultPeriod } from '$lib/hooks/use-book-default-period.svelte';
  import { computePrevRange, PNL_PRESETS } from '$lib/utils/date-utils.svelte';
  import { ReportPeriodPicker as DateRangePicker } from '$lib/components/shared/report-period-picker';
  import { buildTree, flattenAll } from '$lib/features/reports/report-tree';
  import { savingsRate } from '$lib/features/pnl/pnl-utils';

  const meta = PAGE_META.pnl;

  // Shared period state + URL-param restore; must precede useAsyncData so the
  // first fetch already uses values from the URL.
  const range = useReportDateRange();

  useBookDefaultPeriod(range);

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!range.dateFrom && !!range.dateTo && range.rangeValid,
    fetch: () => {
      const { prevFrom, prevTo } = computePrevRange(range.dateFrom, range.dateTo);
      return fetchPnlReport(range.dateFrom, range.dateTo, prevFrom, prevTo);
    },
    label: 'We couldn’t load this report'
  });

  let pnlData = $derived(res.data);

  const trend = useAsyncData({
    enabled: () => !!dbSession.value && !!range.dateFrom && !!range.dateTo && range.rangeValid,
    fetch: () => fetchPnlTrend(range.dateFrom, range.dateTo),
    label: 'We couldn’t load that chart'
  });

  let trendData = $derived(trend.data);
  let trendLoading = $derived(trend.loading);
  let trendError = $derived(trend.error);

  const fmt = formatSessionCurrency;

  let incomeTree = $derived(pnlData ? buildTree(pnlData.incomeRows) : []);
  let expenseTree = $derived(pnlData ? buildTree(pnlData.expenseRows) : []);

  let visibleIncomeRows = $derived(flattenAll(incomeTree));
  let visibleExpenseRows = $derived(flattenAll(expenseTree));

  // Previous-period net income + variance come straight from the backend so
  // the header cards can never drift from row math (same `pnl_change_pct`).
  let prevNetIncome = $derived(pnlData ? pnlData.netIncomePrev : null);
  let netVariance = $derived(
    pnlData && prevNetIncome !== null ? pnlData.netIncome - prevNetIncome : null
  );
  let isZeroVariance = $derived(netVariance === null || netVariance === 0);
  let rate = $derived(pnlData ? savingsRate(pnlData.totalIncome, pnlData.netIncome) : null);
</script>

<svelte:head>
  <title>{meta.title} · Spendo</title>
  <meta name="description" content={meta.description} />
</svelte:head>

<BookGate title={meta.title} description={meta.description}>
  <div class="page-stack">
    <PageHeader title={meta.title} description={meta.description}>
      <DateRangePicker
        bind:value={range.value}
        presets={PNL_PRESETS}
        defaultLabel="Select period"
      />
    </PageHeader>

    {#if !range.rangeValid}
      <ErrorCard
        message="Start date must not be after end date. Adjust the period to generate the report."
      />
    {/if}
    {#if !pnlData && (res.loading || dbBootstrapping.value) && range.rangeValid && !res.error}
      <PageSkeleton variant="pnl" />
    {:else}
      <!-- Net income KPI summary cards -->
      <DimWrapper
        active={res.loading}
        class="grid grid-cols-1 gap-6 @xl:grid-cols-2 @4xl:grid-cols-4"
      >
        <KpiCard
          title="Net income"
          value={pnlData ? fmt(pnlData.netIncome) : EMPTY_DASH}
          valueClass={pnlData ? signColor(pnlData.netIncome) : ''}
          loading={!pnlData && (res.loading || dbBootstrapping.value)}
        >
          {#snippet icon()}
            <ArrowLeftRight aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
        <KpiCard
          title="Savings rate"
          value={rate !== null ? formatAllocPct(rate) : EMPTY_DASH}
          valueClass={rate !== null ? signColor(rate) : 'text-muted-foreground'}
          loading={!pnlData && (res.loading || dbBootstrapping.value)}
        >
          {#snippet icon()}
            <PiggyBank aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
        <KpiCard
          title="Previous period"
          value={pnlData && prevNetIncome !== null ? fmt(prevNetIncome) : EMPTY_DASH}
          loading={!pnlData && (res.loading || dbBootstrapping.value)}
        >
          {#snippet icon()}
            <History aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
        <KpiCard
          title="Variance"
          value={pnlData && netVariance !== null
            ? isZeroVariance
              ? EMPTY_DASH
              : formatSignedSessionCurrency(netVariance)
            : EMPTY_DASH}
          valueClass={pnlData && netVariance !== null && !isZeroVariance
            ? signColor(netVariance)
            : 'text-muted-foreground'}
          loading={!pnlData && (res.loading || dbBootstrapping.value)}
        >
          {#snippet icon()}
            {#if pnlData && netVariance !== null && !isZeroVariance}
              {#if netVariance >= 0}
                <TrendingUp aria-hidden="true" class="size-4 text-success" />
              {:else}
                <TrendingDown aria-hidden="true" class="size-4 text-destructive" />
              {/if}
            {:else}
              <TrendingUp aria-hidden="true" class="size-4 text-muted-foreground" />
            {/if}
          {/snippet}
        </KpiCard>
      </DimWrapper>
      {#if pnlData}
        <MissingFxWarning missing={pnlData.missingFx} reportCurrency={pnlData.reportCurrency} />
      {/if}
      {#if trendData}
        {#if trendError}
          <ErrorCard message={trendError} onRetry={() => trend.reload()} />
        {/if}
        <DimWrapper active={trendLoading}>
          <PnlTrend data={trendData} loading={false} />
        </DimWrapper>
      {:else if trendError}
        <ErrorCard message={trendError} onRetry={() => trend.reload()} />
      {:else if trendData || trendLoading}
        <PnlTrend data={trendData ?? []} loading={trendLoading} />
      {/if}

      <PnlTreetable
        loading={res.loading || dbBootstrapping.value}
        error={res.error}
        onRetry={() => res.reload()}
        {pnlData}
        {visibleIncomeRows}
        {visibleExpenseRows}
      />

      {#if pnlData && !res.loading && !res.error}
        <p class="text-xs text-muted-foreground">
          Amounts converted to {pnlData.reportCurrency || 'report currency'} at month-end rates.
        </p>
      {/if}
    {/if}
  </div>
</BookGate>
