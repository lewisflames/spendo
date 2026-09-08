<script lang="ts" module>
  export type BudgetColumnId =
    'budget' | 'carryIn' | 'spent' | 'expectedYtd' | 'pacing' | 'remaining';

  export const BUDGET_COLUMNS: { id: BudgetColumnId; label: string }[] = [
    { id: 'budget', label: 'Budget' },
    { id: 'carryIn', label: 'Carry-in' },
    { id: 'spent', label: 'Spent' },
    { id: 'expectedYtd', label: 'Expected YTD' },
    { id: 'pacing', label: 'Pacing' },
    { id: 'remaining', label: 'Remaining' }
  ];

  export const BUDGET_DEFAULT_COLUMNS: BudgetColumnId[] = [
    'budget',
    'spent',
    'pacing',
    'remaining'
  ];
</script>

<script lang="ts">
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { cn } from '$lib/utils';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import {
    formatCurrency,
    formatSignedCurrency,
    signColor,
    EMPTY_DASH
  } from '$lib/utils/format.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import { openBudget } from '$lib/utils/navigation';
  import { Plus } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import type { BudgetActualRow } from '$lib/tauri';
  import type { ColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import PacingBar from './pacing-bar.svelte';

  let {
    rows,
    hasBudgets,
    currency,
    year,
    cols,
    onAddClick
  }: {
    rows: BudgetActualRow[] | null;
    hasBudgets: boolean;
    currency: string;
    year: number;
    cols: ColumnVisibility<BudgetColumnId>;
    onAddClick?: () => void;
  } = $props();

  let skeletonCols = $derived(cols.visibleColumns.length + 1);

  function fmt(n: number): string {
    return formatCurrency(n, currency);
  }

  function fmtSigned(n: number): string {
    return formatSignedCurrency(n, currency);
  }
</script>

{#if rows === null}
  <TableSkeleton cols={skeletonCols} rows={4} />
{:else if !hasBudgets}
  <EmptyState
    card
    title="No budgets for {year} yet"
    description="Set a yearly spending target to get started. Years without a budget stay empty."
  >
    {#snippet action()}
      {#if onAddClick}
        <Button onclick={onAddClick}>
          <Plus aria-hidden="true" />
          Add Budget
        </Button>
      {/if}
    {/snippet}
  </EmptyState>
{:else if rows.length === 0}
  <EmptyState card title="No matching budgets" description="Try adjusting your search." />
{:else}
  <TableShell>
    <Table.Header>
      <Table.Row class="bg-muted/75">
        <Table.Head class="text-muted-foreground">Account</Table.Head>
        {#if cols.isVisible('budget')}
          <Table.Head class="text-right text-muted-foreground">Budget</Table.Head>
        {/if}
        {#if cols.isVisible('carryIn')}
          <Table.Head class="text-right text-muted-foreground">Carry-in</Table.Head>
        {/if}
        {#if cols.isVisible('spent')}
          <Table.Head class="text-right text-muted-foreground">Spent</Table.Head>
        {/if}
        {#if cols.isVisible('expectedYtd')}
          <Table.Head class="text-right text-muted-foreground">Expected YTD</Table.Head>
        {/if}
        {#if cols.isVisible('pacing')}
          <Table.Head class="text-muted-foreground">Pacing</Table.Head>
        {/if}
        {#if cols.isVisible('remaining')}
          <Table.Head class="text-right text-muted-foreground">Remaining</Table.Head>
        {/if}
      </Table.Row>
    </Table.Header>
    <Table.Body>
      {#each rows as item (item.guid)}
        <NavRow onNavigate={() => openBudget(item.accountGuid, year)}>
          <Table.Cell class="py-2.5">
            <!-- Accessible link target for keyboard/middle-click; the row click
                 is a convenience shortcut. -->
            <a
              class="block min-w-0 font-medium"
              href="{resolve('/budgets/[accountGuid]', {
                accountGuid: item.accountGuid
              })}?year={year}"
            >
              {item.accountDescription ?? item.accountName}
            </a>
          </Table.Cell>
          {#if cols.isVisible('budget')}
            <Table.Cell class="py-2.5 text-right tabular-nums">
              {fmt(item.budgetAmount)}
            </Table.Cell>
          {/if}
          {#if cols.isVisible('carryIn')}
            <Table.Cell
              class={cn('py-2.5 text-right tabular-nums', signColor(item.rolloverBalance))}
            >
              {fmtSigned(item.rolloverBalance)}
            </Table.Cell>
          {/if}
          {#if cols.isVisible('spent')}
            <Table.Cell class="py-2.5 text-right tabular-nums">
              {fmt(item.actualAmount)}
            </Table.Cell>
          {/if}
          {#if cols.isVisible('expectedYtd')}
            <Table.Cell class="py-2.5 text-right tabular-nums">
              {#if item.elapsedMonths > 0}
                {fmt(item.expectedYtd)}
              {:else}
                <span class="text-xs text-muted-foreground">{EMPTY_DASH}</span>
              {/if}
            </Table.Cell>
          {/if}
          {#if cols.isVisible('pacing')}
            <Table.Cell class="py-2.5">
              {#if item.elapsedMonths > 0}
                <PacingBar
                  pacingPercentage={item.pacingPercentage}
                  actualAmount={item.actualAmount}
                  expectedYtd={item.expectedYtd}
                  delta={item.delta}
                  {fmt}
                />
              {:else}
                <span class="text-xs text-muted-foreground">{EMPTY_DASH}</span>
              {/if}
            </Table.Cell>
          {/if}
          {#if cols.isVisible('remaining')}
            <Table.Cell class={cn('py-2.5 text-right tabular-nums', signColor(item.remaining))}>
              {fmtSigned(item.remaining)}
            </Table.Cell>
          {/if}
        </NavRow>
      {/each}
    </Table.Body>
  </TableShell>
{/if}
