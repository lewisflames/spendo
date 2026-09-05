<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { cn } from '$lib/utils';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { formatCurrency } from '$lib/utils/format.svelte';
  import { handleRowClick, openBudget } from '$lib/utils/navigation';
  import { Plus } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { Progress } from '$lib/components/ui/progress';
  import type { BudgetActualRow } from '$lib/tauri';

  let {
    rows,
    hasBudgets,
    currency,
    year,
    onAddClick
  }: {
    rows: BudgetActualRow[] | null;
    hasBudgets: boolean;
    currency: string;
    year: number;
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
  <TableSkeleton cols={7} rows={4} />
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
        <Table.Head class="text-right text-muted-foreground">Budget</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Carry-in</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Spent</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Expected YTD</Table.Head>
        <Table.Head class="text-muted-foreground">Pacing</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Remaining</Table.Head>
      </Table.Row>
    </Table.Header>
    <Table.Body>
      {#each rows as item (item.guid)}
        <Table.Row
          class="cursor-pointer"
          onclick={(e) => handleRowClick(e, () => openBudget(item.accountGuid, year))}
        >
          <Table.Cell class="py-2.5">
            {item.accountDescription ?? item.accountName}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {fmt(item.budgetAmount)}
          </Table.Cell>
          <Table.Cell
            class={cn('py-2.5 text-right tabular-nums', remainingColor(item.rolloverBalance))}
          >
            {item.rolloverBalance >= 0
              ? `+${fmt(item.rolloverBalance)}`
              : fmt(item.rolloverBalance)}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {fmt(item.actualAmount)}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {#if item.elapsedMonths > 0}
              {fmt(item.expectedYtd)}
            {:else}
              <span class="text-xs text-muted-foreground">&mdash;</span>
            {/if}
          </Table.Cell>
          <Table.Cell class="py-2.5">
            {#if item.elapsedMonths > 0}
              <Tooltip.Root>
                <Tooltip.Trigger>
                  {#snippet child({ props })}
                    <span {...props} class="flex items-center gap-2">
                      <Progress
                        value={Math.min(100, Math.max(0, item.pacingPercentage))}
                        max={100}
                        class={cn(
                          'h-1.5 w-20',
                          item.pacingPercentage > 100 &&
                            'bg-destructive/20 **:data-[slot=progress-indicator]:bg-destructive'
                        )}
                      />
                      <span
                        class={cn(
                          'text-xs tabular-nums',
                          item.pacingPercentage > 100
                            ? 'font-medium text-destructive'
                            : 'text-muted-foreground'
                        )}>{Math.max(0, item.pacingPercentage).toFixed(1)}%</span
                      >
                    </span>
                  {/snippet}
                </Tooltip.Trigger>
                <Tooltip.Content>
                  <p>
                    {fmt(item.actualAmount)} spent of {fmt(item.expectedYtd)} expected
                    {#if item.delta >= 0}
                      ({fmt(item.delta)} under)
                    {:else}
                      ({fmt(-item.delta)} over)
                    {/if}
                  </p>
                </Tooltip.Content>
              </Tooltip.Root>
            {:else}
              <span class="text-xs text-muted-foreground">&mdash;</span>
            {/if}
          </Table.Cell>
          <Table.Cell class={cn('py-2.5 text-right tabular-nums', remainingColor(item.remaining))}>
            {item.remaining >= 0 ? `+${fmt(item.remaining)}` : fmt(item.remaining)}
          </Table.Cell>
        </Table.Row>
      {/each}
    </Table.Body>
  </TableShell>
{/if}
