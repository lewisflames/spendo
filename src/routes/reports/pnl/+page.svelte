<script lang="ts">
  import { dbBootstrapping, dbSession } from '$lib/stores/db.svelte';
  import { fetchPnlReport, fetchPnlTrend } from '$lib/tauri';
  import { signColor, formatSessionCurrency, fmtPct, EMPTY_DASH } from '$lib/utils/format.svelte';
  import { ArrowLeftRight, History, TrendingUp, Percent, Download } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { downloadReportCsv } from '$lib/utils/csv';
  import { toast } from '$lib/components/ui/sonner';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import PnlTreetable from '$lib/features/pnl/pnl-treetable.svelte';
  import PnlTrend from '$lib/features/pnl/pnl-trend.svelte';
  import MissingFxWarning from '$lib/features/reports/missing-fx-warning.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { errMsg } from '$lib/utils/error';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useReportDateRange } from '$lib/hooks/use-report-date-range.svelte';
  import { computePrevRange, PNL_PRESETS } from '$lib/utils/date-utils.svelte';
  import { ReportPeriodPicker as DateRangePicker } from '$lib/components/shared/report-period-picker';
  import { buildTree, flattenAll } from '$lib/features/reports/report-tree';
  import { changePct } from '$lib/features/pnl/pnl-utils';

  // Shared period state + URL-param restore; must precede useAsyncData so the
  // first fetch already uses values from the URL.
  const range = useReportDateRange();

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!range.dateFrom && !!range.dateTo && range.rangeValid,
    fetch: () => {
      const { prevFrom, prevTo } = computePrevRange(range.dateFrom, range.dateTo);
      return fetchPnlReport(range.dateFrom, range.dateTo, prevFrom, prevTo);
    },
    label: 'Failed to load P&L'
  });

  let loading = $derived(res.loading);
  let error = $derived(res.error);
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

  let prevNetIncome = $derived(
    pnlData ? pnlData.totalIncomePrev - pnlData.totalExpensesPrev : null
  );
  let netVariance = $derived(
    pnlData && prevNetIncome !== null ? pnlData.netIncome - prevNetIncome : null
  );
  let netChangePct = $derived(
    pnlData && prevNetIncome !== null ? changePct(pnlData.netIncome, prevNetIncome) : null
  );
  let isZeroVariance = $derived(netVariance === null || netVariance === 0);
  let isZeroChange = $derived(netChangePct === null || netChangePct === 0);

  function handleExportCsv() {
    if (!pnlData) return;
    try {
      const num = (n: number | null | undefined) =>
        n === null || n === undefined ? '' : String(n);
      const section = (type: string, nodes: typeof incomeTree): string[][] =>
        flattenAll(nodes).map(({ node, depth }) => {
          const r = node.row;
          return [
            type,
            r.accountName,
            String(depth),
            String(r.currentAmount),
            String(r.previousAmount),
            String(r.currentAmount - r.previousAmount),
            num(r.changePct)
          ];
        });
      const total = (label: string, current: number, previous: number): string[] => [
        'Total',
        label,
        '',
        String(current),
        String(previous),
        String(current - previous),
        num(changePct(current, previous))
      ];
      downloadReportCsv('pnl.csv', {
        meta: [
          ['Period', `${pnlData.dateFrom} to ${pnlData.dateTo}`],
          ['Previous period', `${pnlData.prevDateFrom} to ${pnlData.prevDateTo}`],
          ['Currency', pnlData.reportCurrency]
        ],
        headers: ['Type', 'Account', 'Depth', 'Current', 'Previous', 'Variance', 'Change %'],
        // Full trees for the export.
        rows: [...section('Income', incomeTree), ...section('Expense', expenseTree)],
        totals: [
          total('Income', pnlData.totalIncome, pnlData.totalIncomePrev),
          total('Expenses', pnlData.totalExpenses, pnlData.totalExpensesPrev),
          total(
            'Net Income',
            pnlData.netIncome,
            pnlData.totalIncomePrev - pnlData.totalExpensesPrev
          )
        ]
      });
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to export P&L'));
    }
  }
</script>

<svelte:head>
  <title>Profit &amp; Loss — Spendo</title>
  <meta name="description" content="Profit and loss statement for a selected period." />
</svelte:head>

<div class="flex flex-col gap-6">
  <PageHeader
    title="Profit & Loss"
    description="Income and expense statement for a selected period"
  >
    <DateRangePicker bind:value={range.value} presets={PNL_PRESETS} defaultLabel="Select period" />
    <Button variant="outline" onclick={handleExportCsv} disabled={!pnlData}>
      <Download aria-hidden="true" class="size-3" />
      Export
    </Button>
  </PageHeader>

  {#if dbSession.value || dbBootstrapping.value}
    {#if !range.rangeValid}
      <p role="alert" class="text-sm text-destructive">
        Start date must not be after end date — adjust the period to generate the report.
      </p>
    {/if}
    <!-- Net income KPI summary cards -->
    <DimWrapper active={loading} class="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-4">
      <KpiCard
        title="Net Income"
        value={pnlData ? fmt(pnlData.netIncome) : EMPTY_DASH}
        valueClass={pnlData ? signColor(pnlData.netIncome) : ''}
        loading={!pnlData && (loading || dbBootstrapping.value)}
      >
        {#snippet icon()}
          <ArrowLeftRight
            aria-hidden="true"
            class="size-4 {pnlData ? signColor(pnlData.netIncome) : 'text-muted-foreground'}"
          />
        {/snippet}
      </KpiCard>
      <KpiCard
        title="Previous Period"
        value={pnlData && prevNetIncome !== null ? fmt(prevNetIncome) : EMPTY_DASH}
        valueClass={pnlData ? 'text-muted-foreground' : ''}
        loading={!pnlData && (loading || dbBootstrapping.value)}
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
            : `${netVariance >= 0 ? '+' : ''}${fmt(netVariance)}`
          : EMPTY_DASH}
        valueClass={pnlData && netVariance !== null && !isZeroVariance
          ? signColor(netVariance)
          : 'text-muted-foreground'}
        loading={!pnlData && (loading || dbBootstrapping.value)}
      >
        {#snippet icon()}
          <TrendingUp
            aria-hidden="true"
            class="size-4 {pnlData && netVariance !== null && !isZeroVariance
              ? signColor(netVariance)
              : 'text-muted-foreground'}"
          />
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
        loading={!pnlData && (loading || dbBootstrapping.value)}
      >
        {#snippet icon()}
          <Percent
            aria-hidden="true"
            class="size-4 {pnlData && netChangePct !== null && !isZeroChange
              ? signColor(netChangePct)
              : 'text-muted-foreground'}"
          />
        {/snippet}
      </KpiCard>
    </DimWrapper>
    {#if pnlData}
      <MissingFxWarning missing={pnlData.missingFx} reportCurrency={pnlData.reportCurrency} />
    {/if}
    {#if trendError && !trendLoading && !trendData}
      <ErrorCard message={trendError} onRetry={() => trend.reload()} />
    {:else if trendData || trendLoading}
      <PnlTrend data={trendData ?? []} loading={trendLoading} />
    {/if}

    <PnlTreetable
      loading={loading || dbBootstrapping.value}
      {error}
      onRetry={() => res.reload()}
      {pnlData}
      {visibleIncomeRows}
      {visibleExpenseRows}
    />

    {#if pnlData && !loading && !error}
      <p class="text-xs text-muted-foreground">
        Amounts converted to {pnlData.reportCurrency || 'report currency'} at month-end rates.
      </p>
    {/if}
  {:else}
    <ImportPrompt />
  {/if}
</div>
