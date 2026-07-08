<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { resolve } from '$app/paths';
  import { signColor } from '$lib/utils/format.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import TreeChevron from '$lib/components/shared/tree-chevron.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { treeIndentStyle } from '$lib/utils';
  import type { ProfitLossReport } from '$lib/tauri';
  import type { SvelteSet } from 'svelte/reactivity';
  import type { PnLTreeNode } from './pnl-types';

  let {
    loading,
    error,
    onRetry,
    pnlData,
    tree,
    visibleIncomeRows,
    visibleExpenseRows,
    fmt,
    fmtPct,
    formatDateRange
  }: {
    loading: boolean;
    error: string | null;
    onRetry?: () => void;
    pnlData: ProfitLossReport | null;
    tree: { expanded: SvelteSet<string>; toggle: (guid: string) => void };
    visibleIncomeRows: { node: PnLTreeNode; depth: number }[];
    visibleExpenseRows: { node: PnLTreeNode; depth: number }[];
    fmt: (n: number) => string;
    fmtPct: (n: number | null) => string;
    formatDateRange: (from: string, to: string) => string;
  } = $props();

  function pct(current: number, previous: number): number | null {
    if (previous !== 0) return ((current - previous) / previous) * 100;
    if (current !== 0) return 0;
    return null;
  }

  // Backend convention (src-tauri/src/reports.rs): income and expense totals
  // are both positive; net = income - expenses.
</script>

{#if loading}
  <TableSkeleton cols={5} rows={4} />
{:else if error}
  <ErrorCard message={error} {onRetry} />
{:else if pnlData}
  {#if pnlData.incomeRows.length === 0 && pnlData.expenseRows.length === 0}
    <EmptyState
      card
      title="No data"
      description="No income or expense transactions found for this period."
    />
  {:else}
    {#snippet pnlRows(rows: { node: PnLTreeNode; depth: number }[], positiveGood: boolean)}
      {#each rows as item (item.node.row.accountGuid)}
        {@const row = item.node.row}
        {@const variance = row.currentAmount - row.previousAmount}
        {@const hasChildren = item.node.children.length > 0}
        {@const isExpanded = tree.expanded.has(row.accountGuid)}
        {@const posClass = signColor(1, positiveGood)}
        {@const negClass = signColor(-1, positiveGood)}
        <Table.Row class={item.depth === 0 ? 'bg-muted/50' : ''}>
          <Table.Cell class="p-0">
            <div class="flex items-center gap-1 py-1.5" style={treeIndentStyle(item.depth)}>
              <TreeChevron
                open={isExpanded}
                interactive={hasChildren}
                ontoggle={() => tree.toggle(row.accountGuid)}
              />
              <a
                href={resolve('/accounts/[guid]', { guid: row.accountGuid })}
                class="min-w-0 truncate text-sm font-medium hover:underline"
              >
                {row.accountDescription ?? row.accountName}
              </a>
            </div>
          </Table.Cell>
          <Table.Cell class="py-1.5 text-right text-sm tabular-nums">
            {fmt(row.currentAmount)}
          </Table.Cell>
          <Table.Cell class="py-1.5 text-right text-sm text-muted-foreground tabular-nums">
            {fmt(row.previousAmount)}
          </Table.Cell>
          <Table.Cell
            class={'py-1.5 text-right text-sm tabular-nums ' +
              (variance !== 0 ? (variance > 0 ? posClass : negClass) : '')}
          >
            {variance >= 0 ? '+' : ''}{fmt(variance)}
          </Table.Cell>
          <Table.Cell
            class={'py-1.5 text-right text-sm tabular-nums ' +
              (row.changePct !== null ? (row.changePct >= 0 ? posClass : negClass) : '')}
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
      {@const changePct = pct(current, previous)}
      {@const posClass = signColor(1, positiveGood)}
      {@const negClass = signColor(-1, positiveGood)}
      <Table.Row class="border-t border-b bg-muted/75 font-semibold">
        <Table.Cell
          class="py-2 text-xs font-semibold tracking-wider text-muted-foreground uppercase"
        >
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
            (variance !== 0 ? (variance > 0 ? posClass : negClass) : '')}
        >
          {variance >= 0 ? '+' : ''}{fmt(variance)}
        </Table.Cell>
        <Table.Cell
          class={'py-2 text-right text-sm tabular-nums ' +
            (changePct !== null ? (changePct >= 0 ? posClass : negClass) : '')}
        >
          {fmtPct(changePct)}
        </Table.Cell>
      </Table.Row>
    {/snippet}

    <TableShell>
      <Table.Header>
        <Table.Row class="bg-muted/75">
          <Table.Head class="w-[30%] text-muted-foreground">Account</Table.Head>
          <Table.Head class="text-right text-muted-foreground">
            Current Period
            <span class="block text-xs font-normal text-muted-foreground/70">
              {formatDateRange(pnlData.dateFrom, pnlData.dateTo)}
            </span>
          </Table.Head>
          <Table.Head class="text-right text-muted-foreground">
            Previous Period
            <span class="block text-xs font-normal text-muted-foreground/70">
              {formatDateRange(pnlData.prevDateFrom, pnlData.prevDateTo)}
            </span>
          </Table.Head>
          <Table.Head class="text-right text-muted-foreground">Variance</Table.Head>
          <Table.Head class="text-right text-muted-foreground">Change</Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#if pnlData.incomeRows.length > 0}
          {@render totalRow(
            'Income',
            pnlData.incomeRootGuid,
            pnlData.totalIncome,
            pnlData.totalIncomePrev,
            true
          )}
          {@render pnlRows(visibleIncomeRows, true)}
        {/if}
        {#if pnlData.expenseRows.length > 0}
          {@render totalRow(
            'Expenses',
            pnlData.expenseRootGuid,
            pnlData.totalExpenses,
            pnlData.totalExpensesPrev,
            false
          )}
          {@render pnlRows(visibleExpenseRows, false)}
        {/if}
        {@render totalRow(
          'Net Income',
          null,
          pnlData.netIncome,
          pnlData.totalIncomePrev - pnlData.totalExpensesPrev,
          true
        )}
      </Table.Body>
    </TableShell>
  {/if}
{:else}
  <EmptyState
    card
    title="No period selected"
    description="Select a period to generate the report."
  />
{/if}
