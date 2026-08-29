<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { cn } from '$lib/utils';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { formatCurrency } from '$lib/utils/format.svelte';
  import { handleRowClick, openBudget } from '$lib/utils/navigation';
  import { Plus } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { Progress } from '$lib/components/ui/progress';
  import type { BudgetRow } from './budget-types';

  let {
    rows,
    hasBudgets,
    currency,
    onAddClick
  }: {
    rows: BudgetRow[] | null;
    hasBudgets: boolean;
    currency: string;
    onAddClick?: () => void;
  } = $props();

  function fmt(n: number): string {
    return formatCurrency(n, currency);
  }

  function remainingColor(remaining: number): string {
    return remaining < 0 ? 'text-destructive' : 'text-success';
  }
</script>

{#if rows === null}
  <TableSkeleton cols={6} rows={4} />
{:else if !hasBudgets}
  <EmptyState
    card
    title="No budgets yet"
    description="Set your first spending target to get started."
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
        <Table.Head class="text-right text-muted-foreground">Budget</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Rollover</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Spent</Table.Head>
        <Table.Head class="text-muted-foreground">Progress</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Remaining</Table.Head>
      </Table.Row>
    </Table.Header>
    <Table.Body>
      {#each rows as item (item.budget.guid)}
        {@const actual = item.actual}
        <Table.Row
          class="cursor-pointer"
          onclick={(e) => handleRowClick(e, () => openBudget(item.budget.accountGuid))}
        >
          <Table.Cell class="py-2.5">
            {item.budget.accountDescription ?? item.budget.accountName}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {#if actual?.hasBudget}
              {fmt(actual.budgetAmount)}
            {:else}
              <span class="text-xs text-muted-foreground">&mdash;</span>
            {/if}
          </Table.Cell>
          <Table.Cell
            class={cn(
              'py-2.5 text-right tabular-nums',
              actual ? remainingColor(actual.rolloverBalance) : 'text-muted-foreground'
            )}
          >
            {actual
              ? actual.rolloverBalance >= 0
                ? `+${fmt(actual.rolloverBalance)}`
                : fmt(actual.rolloverBalance)
              : '\u2014'}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {actual ? fmt(actual.actualAmount) : '\u2014'}
          </Table.Cell>
          <Table.Cell class="py-2.5">
            {#if actual}
              <div class="flex items-center gap-2">
                <Progress
                  value={Math.min(100, Math.max(0, actual.percentage))}
                  max={100}
                  class={cn(
                    'h-1.5 w-20',
                    actual.percentage > 100 &&
                      'bg-destructive/20 **:data-[slot=progress-indicator]:bg-destructive'
                  )}
                />
                <span
                  class={cn(
                    'text-xs tabular-nums',
                    actual.percentage > 100
                      ? 'font-medium text-destructive'
                      : 'text-muted-foreground'
                  )}>{Math.max(0, actual.percentage).toFixed(1)}%</span
                >
              </div>
            {:else}
              <span class="text-xs text-muted-foreground">&mdash;</span>
            {/if}
          </Table.Cell>
          <Table.Cell
            class={cn(
              'py-2.5 text-right tabular-nums',
              actual ? remainingColor(actual.remaining) : 'text-muted-foreground'
            )}
          >
            {actual
              ? actual.remaining >= 0
                ? `+${fmt(actual.remaining)}`
                : fmt(actual.remaining)
              : '\u2014'}
          </Table.Cell>
        </Table.Row>
      {/each}
    </Table.Body>
  </TableShell>
{/if}
