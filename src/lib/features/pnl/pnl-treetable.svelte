<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { resolve } from '$app/paths';
  import {
    formatSessionCurrency as fmt,
    fmtPct,
    formatDateRange,
    signColor,
    EMPTY_DASH
  } from '$lib/utils/format.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import { openAccount } from '$lib/utils/navigation';
  import ReportTable from '$lib/features/reports/report-table.svelte';
  import ReportAccountCell from '$lib/features/reports/report-account-cell.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import * as Tabs from '$lib/components/ui/tabs';
  import type { ProfitLossReport } from '$lib/tauri';
  import type { PnLTreeNode } from './pnl-types';
  import { changePct } from './pnl-utils';

  let {
    loading,
    error,
    onRetry,
    pnlData,
    visibleIncomeRows,
    visibleExpenseRows
  }: {
    loading: boolean;
    error: string | null;
    onRetry?: () => void;
    pnlData: ProfitLossReport | null;
    visibleIncomeRows: { node: PnLTreeNode; depth: number }[];
    visibleExpenseRows: { node: PnLTreeNode; depth: number }[];
  } = $props();

  // Backend convention (src-tauri/src/reports.rs): income and expense totals
  // are both positive; net = income - expenses.
  let section = $state<'income' | 'expenses'>('income');

  let isIncomeEmpty = $derived(pnlData !== null && pnlData.incomeRows.length === 0);
  let isExpensesEmpty = $derived(pnlData !== null && pnlData.expenseRows.length === 0);
  let isEmpty = $derived(section === 'income' ? isIncomeEmpty : isExpensesEmpty);

  // Default to the first non-empty side when a new report arrives so users
  // don't land on an empty tab when only the other side has activity.
  let lastPnlData = $state<ProfitLossReport | null>(null);
  $effect(() => {
    if (pnlData && pnlData !== lastPnlData) {
      lastPnlData = pnlData;
      if (pnlData.incomeRows.length === 0 && pnlData.expenseRows.length > 0) {
        section = 'expenses';
      } else if (pnlData.expenseRows.length === 0 && pnlData.incomeRows.length > 0) {
        section = 'income';
      }
    }
  });
</script>

{#snippet pnlRows(rows: { node: PnLTreeNode; depth: number }[], positiveGood: boolean)}
  {#each rows as item (item.node.row.accountGuid)}
    {@const row = item.node.row}
    {@const variance = row.currentAmount - row.previousAmount}
    {@const isZeroVariance = variance === 0}
    {@const isZeroPct = row.changePct === null || row.changePct === 0}
    <NavRow onNavigate={() => openAccount(row.accountGuid)} class="cursor-pointer hover:bg-muted">
      <ReportAccountCell
        depth={item.depth}
        guid={row.accountGuid}
        name={row.accountName}
        description={row.accountDescription}
      />
      <Table.Cell class="py-2.5 text-right text-sm font-medium whitespace-nowrap tabular-nums">
        {fmt(row.currentAmount)}
      </Table.Cell>
      <Table.Cell
        class="py-2.5 text-right text-sm whitespace-nowrap text-muted-foreground tabular-nums"
      >
        {fmt(row.previousAmount)}
      </Table.Cell>
      <Table.Cell
        class={'py-2.5 text-right text-sm whitespace-nowrap tabular-nums ' +
          (!isZeroVariance ? signColor(variance, positiveGood) : 'text-muted-foreground')}
      >
        {#if isZeroVariance}
          {EMPTY_DASH}
        {:else}
          {variance >= 0 ? '+' : ''}{fmt(variance)}
        {/if}
      </Table.Cell>
      <Table.Cell
        class={'py-2.5 text-right text-sm whitespace-nowrap tabular-nums ' +
          (!isZeroPct && row.changePct !== null
            ? signColor(row.changePct, positiveGood)
            : 'text-muted-foreground')}
      >
        {#if isZeroPct}
          {EMPTY_DASH}
        {:else}
          {fmtPct(row.changePct)}
        {/if}
      </Table.Cell>
    </NavRow>
  {/each}
{/snippet}

{#snippet totalRow(
  label: string,
  guid: string | null,
  current: number,
  previous: number,
  positiveGood: boolean,
  footer = false
)}
  {@const variance = current - previous}
  {@const pct = changePct(current, previous)}
  {@const isZeroVariance = variance === 0}
  {@const isZeroPct = pct === null || pct === 0}
  <NavRow
    onNavigate={guid !== null ? () => openAccount(guid) : null}
    class={footer
      ? guid !== null
        ? 'cursor-pointer border-t-2 bg-muted/50 font-semibold hover:bg-muted'
        : 'border-t-2 bg-muted/50 font-semibold'
      : guid !== null
        ? 'cursor-pointer border-t border-b bg-muted/50 font-semibold hover:bg-muted'
        : 'border-t-2 bg-muted/50 font-semibold'}
  >
    <Table.Cell class="py-2.5 text-xs font-semibold tracking-wider uppercase">
      {#if guid !== null}
        <a
          href={resolve('/accounts/[guid]', { guid })}
          class="focus-visible:outline-2 focus-visible:outline-ring">{label}</a
        >
      {:else}
        {label}
      {/if}
    </Table.Cell>
    <Table.Cell class="py-2.5 text-right text-sm whitespace-nowrap tabular-nums"
      >{fmt(current)}</Table.Cell
    >
    <Table.Cell
      class="py-2.5 text-right text-sm whitespace-nowrap text-muted-foreground tabular-nums"
    >
      {fmt(previous)}
    </Table.Cell>
    <Table.Cell
      class={'py-2.5 text-right text-sm whitespace-nowrap tabular-nums ' +
        (!isZeroVariance ? signColor(variance, positiveGood) : 'text-muted-foreground')}
    >
      {#if isZeroVariance}
        {EMPTY_DASH}
      {:else}
        {variance >= 0 ? '+' : ''}{fmt(variance)}
      {/if}
    </Table.Cell>
    <Table.Cell
      class={'py-2.5 text-right text-sm whitespace-nowrap tabular-nums ' +
        (!isZeroPct && pct !== null ? signColor(pct, positiveGood) : 'text-muted-foreground')}
    >
      {#if isZeroPct}
        {EMPTY_DASH}
      {:else}
        {fmtPct(pct)}
      {/if}
    </Table.Cell>
  </NavRow>
{/snippet}

{#if !pnlData && !loading && !error}
  <EmptyState
    card
    title="No period selected"
    description="Select a period to generate the report."
  />
{:else}
  {#if pnlData}
    <Tabs.Root bind:value={section}>
      <Tabs.List aria-label="Profit and loss section">
        <Tabs.Trigger value="income">Income</Tabs.Trigger>
        <Tabs.Trigger value="expenses">Expenses</Tabs.Trigger>
      </Tabs.List>
    </Tabs.Root>
  {/if}
  <ReportTable
    {loading}
    {error}
    {onRetry}
    hasData={pnlData !== null}
    {isEmpty}
    emptyMessage={section === 'income'
      ? 'No income transactions found for this period.'
      : 'No expense transactions found for this period.'}
    skeletonCols={5}
  >
    {#snippet header()}
      <Table.Header>
        <Table.Row class="bg-muted/75">
          <Table.Head class="w-[30%] text-muted-foreground">Account</Table.Head>
          <Table.Head class="text-right whitespace-nowrap text-muted-foreground">
            Current Period
            <span class="block text-xs font-normal text-muted-foreground/70">
              {formatDateRange(pnlData?.dateFrom, pnlData?.dateTo)}
            </span>
          </Table.Head>
          <Table.Head class="text-right whitespace-nowrap text-muted-foreground">
            Previous Period
            <span class="block text-xs font-normal text-muted-foreground/70">
              {formatDateRange(pnlData?.prevDateFrom, pnlData?.prevDateTo)}
            </span>
          </Table.Head>
          <Table.Head class="text-right whitespace-nowrap text-muted-foreground"
            >Variance</Table.Head
          >
          <Table.Head class="text-right whitespace-nowrap text-muted-foreground">Change</Table.Head>
        </Table.Row>
      </Table.Header>
    {/snippet}
    {#if pnlData && section === 'income'}
      {@render pnlRows(visibleIncomeRows, true)}
      {@render totalRow(
        'Total Income',
        pnlData.incomeRootGuid,
        pnlData.totalIncome,
        pnlData.totalIncomePrev,
        true,
        true
      )}
    {/if}
    {#if pnlData && section === 'expenses'}
      {@render pnlRows(visibleExpenseRows, false)}
      {@render totalRow(
        'Total Expenses',
        pnlData.expenseRootGuid,
        pnlData.totalExpenses,
        pnlData.totalExpensesPrev,
        false,
        true
      )}
    {/if}
  </ReportTable>
{/if}
