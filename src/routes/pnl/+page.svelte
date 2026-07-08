<script lang="ts">
  import { dbSession } from '$lib/stores/db.svelte';
  import { generatePnlReport } from '$lib/tauri';
  import { formatDate, signColor, fmtPct, formatSessionCurrency } from '$lib/utils/format.svelte';
  import { TrendingUp, TrendingDown, ArrowLeftRight, Download } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { downloadCsv } from '$lib/utils/csv';
  import { toast } from '$lib/components/ui/sonner';
  import PageHeader from '$lib/components/layout/page-header.svelte';
  import KpiCard from '$lib/components/shared/kpi-card.svelte';
  import ImportPrompt from '$lib/components/shared/import-prompt.svelte';
  import PnlTreetable from '$lib/features/pnl/pnl-treetable.svelte';
  import { errMsg } from '$lib/utils/error';
  import { useTreeExpand } from '$lib/hooks/use-tree-expand.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import {
    last12MonthsRange,
    computePrevRange,
    PNL_PRESETS,
    calendarDateToStr
  } from '$lib/utils/date-utils.svelte';
  import DateRangePicker from '$lib/components/shared/date-range-picker/date-range-picker.svelte';
  import { useUrlParams } from '$lib/hooks/use-url-params.svelte';
  import { CalendarDate } from '@internationalized/date';
  import type { DateRange } from 'bits-ui';
  import type { ProfitLossRow } from '$lib/tauri';
  import type { PnLTreeNode } from '$lib/features/pnl/pnl-types';

  // Date state + URL-param restore must be set up before useAsyncData so the
  // first fetch already uses values from the URL.
  let calendarRange = $state<DateRange>(last12MonthsRange());

  let pnlDateFrom = $derived(
    calendarRange?.start ? calendarDateToStr(calendarRange.start as CalendarDate) : ''
  );
  let pnlDateTo = $derived(
    calendarRange?.end ? calendarDateToStr(calendarRange.end as CalendarDate) : ''
  );

  function applyCalendarParam(value: string | null, side: 'start' | 'end') {
    if (!value) return;
    const [y, m, d] = value.split('-').map(Number);
    if (!Number.isInteger(y) || !Number.isInteger(m) || !Number.isInteger(d)) return;
    if (m < 1 || m > 12 || d < 1 || d > 31) return;
    const date = new CalendarDate(y, m, d);
    calendarRange = {
      start: side === 'start' ? date : (calendarRange?.start ?? date),
      end: side === 'end' ? date : (calendarRange?.end ?? date)
    };
  }

  useUrlParams([
    {
      key: 'from',
      get: () => pnlDateFrom || null,
      apply: (v) => applyCalendarParam(v, 'start'),
      replace: true
    },
    {
      key: 'to',
      get: () => pnlDateTo || null,
      apply: (v) => applyCalendarParam(v, 'end'),
      replace: true
    }
  ]);

  const res = useAsyncData({
    enabled: () => !!dbSession.value && !!pnlDateFrom && !!pnlDateTo,
    fetch: () => {
      const { prevFrom, prevTo } = computePrevRange(pnlDateFrom, pnlDateTo);
      return generatePnlReport(pnlDateFrom, pnlDateTo, prevFrom, prevTo);
    },
    label: 'Failed to load P&L'
  });

  let loading = $derived(res.loading);
  let error = $derived(res.error);
  let pnlData = $derived(res.data);

  const fmt = formatSessionCurrency;

  function formatDateRange(from: string, to: string): string {
    if (!from || !to) return '';
    try {
      return `${formatDate(from)} – ${formatDate(to)}`;
    } catch {
      return `${from} – ${to}`;
    }
  }

  function buildTree(rows: ProfitLossRow[]): PnLTreeNode[] {
    const root: PnLTreeNode[] = [];
    const stack: { indent: number; node: PnLTreeNode }[] = [];

    for (let i = rows.length - 1; i >= 0; i--) {
      const row = rows[i];
      const node: PnLTreeNode = { row, children: [] };
      while (stack.length > 0 && stack[stack.length - 1].indent >= row.indent) {
        stack.pop();
      }
      if (stack.length === 0) {
        root.push(node);
      } else {
        stack[stack.length - 1].node.children.push(node);
      }
      stack.push({ indent: row.indent, node });
    }

    function reverseTree(nodes: PnLTreeNode[]) {
      nodes.reverse();
      for (const n of nodes) {
        reverseTree(n.children);
      }
    }
    reverseTree(root);

    return root;
  }

  function flattenTree(
    nodes: PnLTreeNode[],
    expanded: Set<string>,
    depth = 0
  ): { node: PnLTreeNode; depth: number }[] {
    const result: { node: PnLTreeNode; depth: number }[] = [];
    for (const n of nodes) {
      result.push({ node: n, depth });
      if (n.children.length > 0 && expanded.has(n.row.accountGuid)) {
        result.push(...flattenTree(n.children, expanded, depth + 1));
      }
    }
    return result;
  }

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
      tree.collapseAll();
      for (const g of collectParentGuids(incomeTree)) {
        tree.expanded.add(g);
      }
      for (const g of collectParentGuids(expenseTree)) {
        tree.expanded.add(g);
      }
    }
  });

  function collectParentGuids(nodes: PnLTreeNode[]): string[] {
    const guids: string[] = [];
    for (const n of nodes) {
      if (n.children.length > 0) {
        guids.push(n.row.accountGuid);
        guids.push(...collectParentGuids(n.children));
      }
    }
    return guids;
  }

  let visibleIncomeRows = $derived(flattenTree(incomeTree, tree.expanded));
  let visibleExpenseRows = $derived(flattenTree(expenseTree, tree.expanded));

  function handleExportCsv() {
    if (!pnlData) return;
    try {
      const headers = ['Type', 'Account', 'Depth', 'Current', 'Previous', 'Change %'];
      const rows = [
        ...visibleIncomeRows.map(({ node, depth }) => ({
          type: 'Income',
          node,
          depth
        })),
        ...visibleExpenseRows.map(({ node, depth }) => ({ type: 'Expense', node, depth }))
      ].map(({ type, node, depth }) => [
        type,
        node.row.accountName,
        String(depth),
        String(node.row.currentAmount),
        String(node.row.previousAmount),
        node.row.changePct === null || node.row.changePct === undefined
          ? ''
          : String(node.row.changePct)
      ]);
      downloadCsv('pnl.csv', headers, rows);
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
    <DateRangePicker
      bind:value={calendarRange}
      presets={PNL_PRESETS}
      defaultLabel="Select period"
    />
    <Button variant="outline" onclick={handleExportCsv} disabled={!pnlData}>
      <Download aria-hidden="true" class="size-3" />
      Export
    </Button>
  </PageHeader>

  {#if dbSession.value}
    <!-- KPI summary cards -->
    {#if pnlData && !loading}
      {@const pd = pnlData}
      <div class="grid grid-cols-1 gap-6 sm:grid-cols-3">
        <KpiCard title="Total Income" value={fmt(pd.totalIncome)} valueClass="text-success">
          {#snippet icon()}
            <TrendingUp aria-hidden="true" class="size-4 text-success" />
          {/snippet}
        </KpiCard>
        <KpiCard title="Total Expenses" value={fmt(pd.totalExpenses)} valueClass="text-destructive">
          {#snippet icon()}
            <TrendingDown aria-hidden="true" class="size-4 text-destructive" />
          {/snippet}
        </KpiCard>
        <KpiCard title="Net Income" value={fmt(pd.netIncome)} valueClass={signColor(pd.netIncome)}>
          {#snippet icon()}
            <ArrowLeftRight aria-hidden="true" class="size-4 {signColor(pd.netIncome)}" />
          {/snippet}
        </KpiCard>
      </div>
    {/if}

    <PnlTreetable
      {loading}
      {error}
      onRetry={() => res.reload()}
      {pnlData}
      {tree}
      {visibleIncomeRows}
      {visibleExpenseRows}
      {fmt}
      {fmtPct}
      {formatDateRange}
    />
  {:else}
    <ImportPrompt />
  {/if}
</div>
