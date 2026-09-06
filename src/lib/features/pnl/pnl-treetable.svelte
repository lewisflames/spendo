<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { resolve } from '$app/paths';
  import {
    formatSessionCurrency as fmt,
    fmtPct,
    formatDateRange,
    signColor
  } from '$lib/utils/format.svelte';
  import ReportTable from '$lib/features/reports/report-table.svelte';
  import ReportAccountCell from '$lib/features/reports/report-account-cell.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import type { ProfitLossReport } from '$lib/tauri';
  import type { SvelteSet } from 'svelte/reactivity';
  import type { PnLTreeNode } from './pnl-types';
  import { changePct } from './pnl-utils';

  let {
    loading,
    error,
    onRetry,
    pnlData,
    tree,
    visibleIncomeRows,
    visibleExpenseRows
  }: {
    loading: boolean;
    error: string | null;
    onRetry?: () => void;
    pnlData: ProfitLossReport | null;
    tree: { expanded: SvelteSet<string>; toggle: (guid: string) => void };
    visibleIncomeRows: { node: PnLTreeNode; depth: number }[];
    visibleExpenseRows: { node: PnLTreeNode; depth: number }[];
  } = $props();

  // Backend convention (src-tauri/src/reports.rs): income and expense totals
  // are both positive; net = income - expenses.
  let isEmpty = $derived(
    pnlData !== null && pnlData.incomeRows.length === 0 && pnlData.expenseRows.length === 0
  );
</script>

{#snippet pnlRows(rows: { node: PnLTreeNode; depth: number }[], positiveGood: boolean)}
  {#each rows as item (item.node.row.accountGuid)}
    {@const row = item.node.row}
    {@const variance = row.currentAmount - row.previousAmount}
    <Table.Row class={item.depth === 0 ? 'bg-muted/50' : ''}>
      <ReportAccountCell
        depth={item.depth}
        guid={row.accountGuid}
        name={row.accountName}
        description={row.accountDescription}
        hasChildren={item.node.children.length > 0}
        expanded={tree.expanded.has(row.accountGuid)}
        onToggle={() => tree.toggle(row.accountGuid)}
      />
      <Table.Cell class="py-1.5 text-right text-sm tabular-nums">
        {fmt(row.currentAmount)}
      </Table.Cell>
      <Table.Cell class="py-1.5 text-right text-sm text-muted-foreground tabular-nums">
        {fmt(row.previousAmount)}
      </Table.Cell>
      <Table.Cell
        class={'py-1.5 text-right text-sm tabular-nums ' +
          (variance !== 0 ? signColor(variance, positiveGood) : '')}
      >
        {variance >= 0 ? '+' : ''}{fmt(variance)}
      </Table.Cell>
      <Table.Cell
        class={'py-1.5 text-right text-sm tabular-nums ' +
          (row.changePct !== null ? signColor(row.changePct, positiveGood) : '')}
      >
        {fmtPct(row.changePct)}
      </Table.Cell>
    </Table.Row>
  {/each}
{/snippet}

{#snippet totalRow(
  label: string,
  guid: string | null,
  current: number,
  previous: number,
  positiveGood: boolean
)}
  {@const variance = current - previous}
  {@const pct = changePct(current, previous)}
  <Table.Row class="border-t border-b bg-muted/75 font-semibold">
    <Table.Cell class="py-2 text-xs font-semibold tracking-wider text-muted-foreground uppercase">
      {#if guid !== null}
        <a href={resolve('/accounts/[guid]', { guid })} class="hover:underline">{label}</a>
      {:else}
        {label}
      {/if}
    </Table.Cell>
    <Table.Cell class="py-2 text-right text-sm tabular-nums">{fmt(current)}</Table.Cell>
    <Table.Cell class="py-2 text-right text-sm text-muted-foreground tabular-nums">
      {fmt(previous)}
    </Table.Cell>
    <Table.Cell
      class={'py-2 text-right text-sm tabular-nums ' +
        (variance !== 0 ? signColor(variance, positiveGood) : '')}
    >
      {variance >= 0 ? '+' : ''}{fmt(variance)}
    </Table.Cell>
    <Table.Cell
      class={'py-2 text-right text-sm tabular-nums ' +
        (pct !== null ? signColor(pct, positiveGood) : '')}
    >
      {fmtPct(pct)}
    </Table.Cell>
  </Table.Row>
{/snippet}

{#if !pnlData && !loading && !error}
  <EmptyState
    card
    title="No period selected"
    description="Select a period to generate the report."
  />
{:else}
  <ReportTable
    {loading}
    {error}
    {onRetry}
    hasData={pnlData !== null}
    {isEmpty}
    emptyMessage="No income or expense transactions found for this period."
    skeletonCols={5}
  >
    {#snippet header()}
      <Table.Header>
        <Table.Row class="bg-muted/75">
          <Table.Head class="w-[30%] text-muted-foreground">Account</Table.Head>
          <Table.Head class="text-right text-muted-foreground">
            Current Period
            <span class="block text-xs font-normal text-muted-foreground/70">
              {formatDateRange(pnlData?.dateFrom, pnlData?.dateTo)}
            </span>
          </Table.Head>
          <Table.Head class="text-right text-muted-foreground">
            Previous Period
            <span class="block text-xs font-normal text-muted-foreground/70">
              {formatDateRange(pnlData?.prevDateFrom, pnlData?.prevDateTo)}
            </span>
          </Table.Head>
          <Table.Head class="text-right text-muted-foreground">Variance</Table.Head>
          <Table.Head class="text-right text-muted-foreground">Change</Table.Head>
        </Table.Row>
      </Table.Header>
    {/snippet}
    {#if pnlData && pnlData.incomeRows.length > 0}
      {@render totalRow(
        'Income',
        pnlData.incomeRootGuid,
        pnlData.totalIncome,
        pnlData.totalIncomePrev,
        true
      )}
      {@render pnlRows(visibleIncomeRows, true)}
    {/if}
    {#if pnlData && pnlData.expenseRows.length > 0}
      {@render totalRow(
        'Expenses',
        pnlData.expenseRootGuid,
        pnlData.totalExpenses,
        pnlData.totalExpensesPrev,
        false
      )}
      {@render pnlRows(visibleExpenseRows, false)}
    {/if}
    {#if pnlData}
      {@render totalRow(
        'Net Income',
        null,
        pnlData.netIncome,
        pnlData.totalIncomePrev - pnlData.totalExpensesPrev,
        true
      )}
    {/if}
  </ReportTable>
{/if}
