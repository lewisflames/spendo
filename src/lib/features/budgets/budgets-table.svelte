<script lang="ts">
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { cn } from '$lib/utils';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { formatCurrency, formatSignedCurrency } from '$lib/utils/format.svelte';
  import { EMPTY_DASH } from '$lib/utils/format.svelte';
  import { handleRowClick, openBudget } from '$lib/utils/navigation';
  import { Plus } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import type { BudgetActualRow } from '$lib/tauri';
  import PacingBar from './pacing-bar.svelte';
  import { remainingColor } from './budget-utils';

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

  function fmtSigned(n: number): string {
    return formatSignedCurrency(n, currency);
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
          class="cursor-pointer hover:bg-muted/50"
          onclick={(e) => handleRowClick(e, () => openBudget(item.accountGuid, year))}
        >
          <Table.Cell class="py-2.5">
            <!-- Accessible link target for keyboard/middle-click; the row click
                 is a convenience shortcut. -->
            <a
              class="block min-w-0 font-medium hover:underline"
              href="{resolve('/budgets/[accountGuid]', {
                accountGuid: item.accountGuid
              })}?year={year}"
            >
              {item.accountDescription ?? item.accountName}
            </a>
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {fmt(item.budgetAmount)}
          </Table.Cell>
          <Table.Cell
            class={cn('py-2.5 text-right tabular-nums', remainingColor(item.rolloverBalance))}
          >
            {fmtSigned(item.rolloverBalance)}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {fmt(item.actualAmount)}
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            {#if item.elapsedMonths > 0}
              {fmt(item.expectedYtd)}
            {:else}
              <span class="text-xs text-muted-foreground">{EMPTY_DASH}</span>
            {/if}
          </Table.Cell>
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
          <Table.Cell class={cn('py-2.5 text-right tabular-nums', remainingColor(item.remaining))}>
            {fmtSigned(item.remaining)}
          </Table.Cell>
        </Table.Row>
      {/each}
    </Table.Body>
  </TableShell>
{/if}
