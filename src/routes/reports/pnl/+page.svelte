<script lang="ts">
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { untrack } from 'svelte';
  import { fetchPnlReport, fetchPnlTrend } from '$lib/tauri';
  import {
    signColor,
    formatSessionCurrency,
    formatSignedSessionCurrency,
    fmtPct,
    EMPTY_DASH
  } from '$lib/utils/format.svelte';
  import { ArrowLeftRight, History, TrendingUp, Percent } from '@lucide/svelte';
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
  import { computePrevRange, PNL_PRESETS, getPnlPeriodRange } from '$lib/utils/date-utils.svelte';
  import { loadBookSettings } from '$lib/stores/book-settings.svelte';
  import { page } from '$app/state';
  import { ReportPeriodPicker as DateRangePicker } from '$lib/components/shared/report-period-picker';
  import { buildTree, flattenAll } from '$lib/features/reports/report-tree';

  // Shared period state + URL-param restore; must precede useAsyncData so the
  // first fetch already uses values from the URL.
  const range = useReportDateRange();

  // Book default period (FY-aware) applies once on first load when the URL
  // carries no explicit dates. Explicit ?from/?to always win; later user
  // picks never get clobbered.
  let defaultPeriodApplied = $state(false);
  $effect(() => {
    if (!dbSession.value || defaultPeriodApplied) return;
    // Snapshot the URL-restored range: if the user picks a new period while
    // settings load, the snapshot differs and the default is skipped (the URL
    // guard alone loses the race because goto() updates the URL asynchronously).
    const rangeAtStart = untrack(() => `${range.dateFrom}|${range.dateTo}`);
    loadBookSettings()
      .then((settings) => {
        if (defaultPeriodApplied) return;
        const sp = page.url.searchParams;
        if (
          sp.get('from') ||
          sp.get('to') ||
          `${range.dateFrom}|${range.dateTo}` !== rangeAtStart
        ) {
          defaultPeriodApplied = true;
          return;
        }
        const { start, end } = getPnlPeriodRange(
          settings.defaultPnlPeriod,
          settings.fiscalYearStartMonth
        );
        range.value = { start, end };
        defaultPeriodApplied = true;
      })
      .catch(() => {
        defaultPeriodApplied = true;
      });
  });

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!range.dateFrom && !!range.dateTo && range.rangeValid,
    fetch: () => {
      const { prevFrom, prevTo } = computePrevRange(range.dateFrom, range.dateTo);
      return fetchPnlReport(range.dateFrom, range.dateTo, prevFrom, prevTo);
    },
    label: 'Failed to load P&L'
  });

  let pnlData = $derived(res.data);

  const trend = useAsyncData({
    enabled: () => !!dbSession.value && !!range.dateFrom && !!range.dateTo && range.rangeValid,
    fetch: () => fetchPnlTrend(range.dateFrom, range.dateTo),
    label: 'Failed to load P&L trend'
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
  let netChangePct = $derived(pnlData ? pnlData.netIncomeChangePct : null);
  let isZeroVariance = $derived(netVariance === null || netVariance === 0);
  let isZeroChange = $derived(netChangePct === null || netChangePct === 0);
</script>

<svelte:head>
  <title>Profit &amp; Loss — Spendo</title>
  <meta name="description" content="Profit and loss statement for a selected period." />
</svelte:head>

<BookGate title="Profit & Loss" description="Income and expense statement for a selected period">
  <div class="flex flex-col gap-6">
    <PageHeader
      title="Profit & Loss"
      description="Income and expense statement for a selected period"
    >
      <DateRangePicker
        bind:value={range.value}
        presets={PNL_PRESETS}
        defaultLabel="Select period"
      />
    </PageHeader>

    {#if !range.rangeValid}
      <ErrorCard
        message="Start date must not be after end date — adjust the period to generate the report."
      />
    {/if}
    {#if !pnlData && (res.loading || dbBootstrapping.value) && range.rangeValid && !res.error}
      <PageSkeleton variant="pnl" />
    {:else}
      <!-- Net income KPI summary cards -->
      <DimWrapper active={res.loading} class="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-4">
        <KpiCard
          title="Net Income"
          value={pnlData ? fmt(pnlData.netIncome) : EMPTY_DASH}
          valueClass={pnlData ? signColor(pnlData.netIncome) : ''}
          loading={!pnlData && (res.loading || dbBootstrapping.value)}
        >
          {#snippet icon()}
            <ArrowLeftRight aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
        <KpiCard
          title="Previous Period"
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
            <TrendingUp aria-hidden="true" class="size-4 text-muted-foreground" />
          {/snippet}
        </KpiCard>
        <KpiCard
          title="Change"
          value={pnlData && netChangePct !== null
            ? isZeroChange
              ? EMPTY_DASH
              : fmtPct(netChangePct)
            : EMPTY_DASH}
          valueClass={pnlData && netChangePct !== null && !isZeroChange
            ? signColor(netChangePct)
            : 'text-muted-foreground'}
          loading={!pnlData && (res.loading || dbBootstrapping.value)}
        >
          {#snippet icon()}
            <Percent aria-hidden="true" class="size-4 text-muted-foreground" />
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
