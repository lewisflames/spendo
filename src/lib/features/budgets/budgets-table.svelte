<script lang="ts" module>
  export type BudgetColumnId =
    | 'budget'
    | 'monthly'
    | 'carryIn'
    | 'spent'
    | 'expectedYtd'
    | 'aheadBehind'
    | 'pacing'
    | 'remaining';

  export const BUDGET_COLUMNS: { id: BudgetColumnId; label: string }[] = [
    { id: 'budget', label: 'Annual' },
    { id: 'monthly', label: 'Monthly' },
    { id: 'carryIn', label: 'Carry-in' },
    { id: 'spent', label: 'Spent' },
    { id: 'expectedYtd', label: 'Expected YTD' },
    { id: 'aheadBehind', label: 'Ahead / behind' },
    { id: 'pacing', label: 'Pacing' },
    { id: 'remaining', label: 'Remaining' }
  ];

  export const BUDGET_DEFAULT_COLUMNS: BudgetColumnId[] = [
    'budget',
    'monthly',
    'spent',
    'aheadBehind',
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
  import FilteredEmptyState from '$lib/components/shared/filtered-empty-state.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import AccountLabel from '$lib/features/accounts/account-label.svelte';
  import { formatCurrency, formatSignedCurrency, signColor } from '$lib/utils/format.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import { openBudget } from '$lib/utils/navigation';
  import { PiggyBank, Plus } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import type { BudgetActualRow } from '$lib/tauri';
  import type { ColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import PacingBar from './pacing-bar.svelte';
  import { budgetTotals } from './budget-utils';

  let {
    rows,
    hasBudgets,
    currency,
    year,
    cols,
    onAddClick,
    onClearSearch
  }: {
    rows: BudgetActualRow[] | null;
    hasBudgets: boolean;
    currency: string;
    year: number;
    cols: ColumnVisibility<BudgetColumnId>;
    onAddClick?: () => void;
    onClearSearch: () => void;
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
    title="Nothing budgeted for {year} yet"
    description="Set a yearly target and we’ll track your spending against it."
  >
    {#snippet icon()}
      <PiggyBank aria-hidden="true" class="text-muted-foreground" />
    {/snippet}
    {#snippet action()}
      {#if onAddClick}
        <Button onclick={onAddClick}>
          <Plus aria-hidden="true" class="size-4" />
          New budget
        </Button>
      {/if}
    {/snippet}
  </EmptyState>
{:else if rows.length === 0}
  <FilteredEmptyState title="No matching budgets" onClear={onClearSearch} />
{:else}
  {@const totals = budgetTotals(rows)}
  <TableShell>
    <Table.Header>
      <Table.Row class="bg-muted/75">
        <Table.Head class="text-muted-foreground">Account</Table.Head>
        {#if cols.isVisible('budget')}
          <Table.Head class="text-right text-muted-foreground">Annual</Table.Head>
        {/if}
        {#if cols.isVisible('monthly')}
          <Table.Head class="text-right text-muted-foreground">Monthly</Table.Head>
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
        {#if cols.isVisible('aheadBehind')}
          <Table.Head class="text-right text-muted-foreground">Ahead / behind</Table.Head>
        {/if}
        {#if cols.isVisible('pacing')}
          <Table.Head class="text-left text-muted-foreground">Pacing</Table.Head>
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
              class="block min-w-0 focus-visible:outline-2 focus-visible:outline-ring"
              href="{resolve('/budgets/[accountGuid]', {
                accountGuid: item.accountGuid
              })}?year={year}"
            >
              <AccountLabel
                name={item.accountName}
                description={item.accountDescription ?? null}
                code={item.accountCode}
              />
            </a>
          </Table.Cell>
          {#if cols.isVisible('budget')}
            <Table.Cell class="py-2.5 text-right tabular-nums">
              {fmt(item.budgetAmount)}
            </Table.Cell>
          {/if}
          {#if cols.isVisible('monthly')}
            <Table.Cell class="py-2.5 text-right tabular-nums">
              {fmt(item.budgetAmount / 12)}
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
                <EmptyValue />
              {/if}
            </Table.Cell>
          {/if}
          {#if cols.isVisible('aheadBehind')}
            <Table.Cell
              class={cn(
                'py-2.5 text-right tabular-nums',
                item.elapsedMonths > 0 && signColor(item.delta)
              )}
            >
              {#if item.elapsedMonths > 0}
                {fmtSigned(item.delta)}
              {:else}
                <EmptyValue />
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
                <EmptyValue />
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
    <Table.Footer>
      <Table.Row class="border-t bg-muted/50 font-semibold">
        <Table.Cell class="py-2.5 text-xs font-semibold tracking-wider uppercase">Total</Table.Cell>
        {#if cols.isVisible('budget')}
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {fmt(totals.budgetAmount)}
          </Table.Cell>
        {/if}
        {#if cols.isVisible('monthly')}
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {fmt(totals.monthlyAmount)}
          </Table.Cell>
        {/if}
        {#if cols.isVisible('carryIn')}
          <Table.Cell
            class={cn('py-2.5 text-right tabular-nums', signColor(totals.rolloverBalance))}
          >
            {fmtSigned(totals.rolloverBalance)}
          </Table.Cell>
        {/if}
        {#if cols.isVisible('spent')}
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {fmt(totals.actualAmount)}
          </Table.Cell>
        {/if}
        {#if cols.isVisible('expectedYtd')}
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {#if totals.elapsedMonths > 0}
              {fmt(totals.expectedYtd)}
            {:else}
              <EmptyValue />
            {/if}
          </Table.Cell>
        {/if}
        {#if cols.isVisible('aheadBehind')}
          <Table.Cell
            class={cn(
              'py-2.5 text-right tabular-nums',
              totals.elapsedMonths > 0 && signColor(totals.delta)
            )}
          >
            {#if totals.elapsedMonths > 0}
              {fmtSigned(totals.delta)}
            {:else}
              <EmptyValue />
            {/if}
          </Table.Cell>
        {/if}
        {#if cols.isVisible('pacing')}
          <Table.Cell class="py-2.5">
            {#if totals.elapsedMonths > 0}
              <PacingBar
                pacingPercentage={totals.pacingPercentage}
                actualAmount={totals.actualAmount}
                expectedYtd={totals.expectedYtd}
                delta={totals.delta}
                {fmt}
              />
            {:else}
              <EmptyValue />
            {/if}
          </Table.Cell>
        {/if}
        {#if cols.isVisible('remaining')}
          <Table.Cell class={cn('py-2.5 text-right tabular-nums', signColor(totals.remaining))}>
            {fmtSigned(totals.remaining)}
          </Table.Cell>
        {/if}
      </Table.Row>
    </Table.Footer>
  </TableShell>
{/if}
