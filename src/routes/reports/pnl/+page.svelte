<script lang="ts">
  import { resolve } from '$app/paths';
  import { dbSession } from '$lib/stores/db.svelte';
  import { generatePnlReport, generatePnlTrend } from '$lib/tauri';
  import { signColor, formatSessionCurrency, EMPTY_DASH } from '$lib/utils/format.svelte';
  import { TrendingUp, TrendingDown, ArrowLeftRight, Download } from '@lucide/svelte';
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
  import { errMsg } from '$lib/utils/error';
  import { useTreeExpand } from '$lib/hooks/use-tree-expand.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useReportDateRange } from '$lib/hooks/use-report-date-range.svelte';
  import { computePrevRange, PNL_PRESETS } from '$lib/utils/date-utils.svelte';
  import DateRangePicker from '$lib/components/shared/report-period-picker/report-period-picker.svelte';
  import {
    buildTree,
    flattenTree,
    flattenAll,
    collectParentGuids
  } from '$lib/features/reports/report-tree';
  import { changePct } from '$lib/features/pnl/pnl-utils';

  // Shared period state + URL-param restore; must precede useAsyncData so the
  // first fetch already uses values from the URL.
  const range = useReportDateRange();

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!range.dateFrom && !!range.dateTo && range.rangeValid,
    fetch: () => {
      const { prevFrom, prevTo } = computePrevRange(range.dateFrom, range.dateTo);
      return generatePnlReport(range.dateFrom, range.dateTo, prevFrom, prevTo);
    },
    label: 'Failed to load P&L'
  });

  let loading = $derived(res.loading);
  let error = $derived(res.error);
  let pnlData = $derived(res.data);

  const trend = useAsyncData({
    enabled: () => !!dbSession.value && !!range.dateFrom && !!range.dateTo && range.rangeValid,
    fetch: () => generatePnlTrend(range.dateFrom, range.dateTo),
    label: 'Failed to load P&L trend'
  });

  let trendData = $derived(trend.data);
  let trendLoading = $derived(trend.loading);

  const fmt = formatSessionCurrency;

  let tree = useTreeExpand();
  let appliedExpandKey = '';

  let incomeTree = $derived(pnlData ? buildTree(pnlData.incomeRows) : []);
  let expenseTree = $derived(pnlData ? buildTree(pnlData.expenseRows) : []);

  $effect(() => {
    if (incomeTree.length > 0 || expenseTree.length > 0) {
      // Only reset expansions when the report's account structure changes;
      // plain refetches (e.g. a nudged date range) must keep the user's
      // manual collapse choices.
      const key =
        collectParentGuids(incomeTree).join(',') + '|' + collectParentGuids(expenseTree).join(',');
      if (key === appliedExpandKey) return;
      appliedExpandKey = key;
      tree.expandAll([...collectParentGuids(incomeTree), ...collectParentGuids(expenseTree)]);
    }
  });

  let visibleIncomeRows = $derived(flattenTree(incomeTree, tree.expanded));
  let visibleExpenseRows = $derived(flattenTree(expenseTree, tree.expanded));

  function expandAllPnl() {
    tree.expandAll([...collectParentGuids(incomeTree), ...collectParentGuids(expenseTree)]);
  }

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
        // Full trees regardless of UI collapse state.
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

  {#if dbSession.value}
    {#if !range.rangeValid}
      <p role="alert" class="text-sm text-destructive">
        Start date must not be after end date — adjust the period to generate the report.
      </p>
    {/if}
    <!-- KPI summary cards -->
    <DimWrapper active={loading} class="grid grid-cols-1 gap-6 sm:grid-cols-3">
      <KpiCard
        title="Total Income"
        value={pnlData ? fmt(pnlData.totalIncome) : EMPTY_DASH}
        valueClass={pnlData ? 'text-success' : ''}
        loading={!pnlData && loading}
      >
        {#snippet icon()}
          <TrendingUp aria-hidden="true" class="size-4 text-success" />
        {/snippet}
      </KpiCard>
      <KpiCard
        title="Total Expenses"
        value={pnlData ? fmt(pnlData.totalExpenses) : EMPTY_DASH}
        valueClass={pnlData ? 'text-destructive' : ''}
        loading={!pnlData && loading}
      >
        {#snippet icon()}
          <TrendingDown aria-hidden="true" class="size-4 text-destructive" />
        {/snippet}
      </KpiCard>
      <KpiCard
        title="Net Income"
        value={pnlData ? fmt(pnlData.netIncome) : EMPTY_DASH}
        valueClass={pnlData ? signColor(pnlData.netIncome) : ''}
        loading={!pnlData && loading}
      >
        {#snippet icon()}
          <ArrowLeftRight
            aria-hidden="true"
            class="size-4 {pnlData ? signColor(pnlData.netIncome) : 'text-muted-foreground'}"
          />
        {/snippet}
      </KpiCard>
    </DimWrapper>
    {#if pnlData}
      <MissingFxWarning missing={pnlData.missingFx} reportCurrency={pnlData.reportCurrency} />
    {/if}
    {#if trendData || trendLoading}
      <PnlTrend data={trendData ?? []} loading={trendLoading} />
    {/if}
    <div class="flex items-center justify-end gap-1">
      <Button variant="ghost" size="sm" onclick={expandAllPnl} disabled={!pnlData}>
        Expand all
      </Button>
      <Button variant="ghost" size="sm" onclick={() => tree.collapseAll()} disabled={!pnlData}>
        Collapse all
      </Button>
    </div>

    <PnlTreetable
      {loading}
      {error}
      onRetry={() => res.reload()}
      {pnlData}
      {tree}
      {visibleIncomeRows}
      {visibleExpenseRows}
    />

    {#if pnlData && !loading && !error}
      <p class="text-xs text-muted-foreground">
        Amounts converted to {pnlData.reportCurrency || 'report currency'} at month-end rates.
        <a
          href="{resolve('/transactions')}?from={range.dateFrom}&to={range.dateTo}"
          class="font-medium underline"
        >
          View transactions for this period
        </a>.
      </p>
    {/if}
  {:else}
    <ImportPrompt />
  {/if}
</div>
