<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Table from '$lib/components/ui/table';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { cn } from '$lib/utils';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { formatCurrency } from '$lib/utils/format.svelte';
  import { Plus, Trash2, Pencil, StickyNote } from '@lucide/svelte';
  import type { Budget } from '$lib/tauri';
  import type { BudgetRow, BudgetGroup } from './budget-types';
  import { resolve } from '$app/paths';
  let {
    groups,
    hasBudgets,
    currency,
    viewMode,
    onEdit,
    onDeleteClick,
    onAddClick
  }: {
    groups: BudgetGroup[] | null;
    hasBudgets: boolean;
    currency: string;
    viewMode: 'MONTHLY' | 'YEARLY';
    onEdit?: (budget: Budget) => void;
    onDeleteClick?: (guid: string, name: string) => void;
    onAddClick?: () => void;
  } = $props();

  function fmt(n: number): string {
    return formatCurrency(n, currency);
  }

  function remainingColor(remaining: number): string {
    return remaining < 0 ? 'text-destructive' : 'text-success';
  }

  function groupTotals(rows: BudgetRow[]) {
    const target = rows.reduce((s, r) => s + (r.actual?.budgetAmount ?? r.budget.amount), 0);
    const rollover = rows.reduce((s, r) => s + (r.actual?.rolloverBalance ?? 0), 0);
    const spent = rows.reduce((s, r) => s + (r.actual?.actualAmount ?? 0), 0);
    const remaining = rows.reduce((s, r) => s + (r.actual?.remaining ?? 0), 0);
    const effective = target + rollover;
    const pct = effective > 0 ? (Math.max(0, spent) / effective) * 100 : 0;
    return { target, rollover, spent, remaining, pct };
  }
</script>

{#if groups === null}
  <TableSkeleton cols={7} rows={4} />
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
{:else if groups.length === 0}
  <EmptyState card title="No matching budgets" description="Try adjusting your search." />
{:else}
  <TableShell>
    <Table.Header>
      <Table.Row class="bg-muted/75">
        <Table.Head class="text-muted-foreground">Account</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Base</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Target</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Spent</Table.Head>
        <Table.Head class="text-muted-foreground">Progress</Table.Head>
        <Table.Head class="text-right text-muted-foreground">Remaining</Table.Head>
        <Table.Head class="w-16"><span class="sr-only">Actions</span></Table.Head>
      </Table.Row>
    </Table.Header>
    <Table.Body>
      {#each groups as group (group.category)}
        {@const gt = groupTotals(group.rows)}
        <Table.Row class="bg-muted/40">
          <Table.Cell class="py-2.5">
            <span class="text-xs font-semibold tracking-wide text-muted-foreground uppercase"
              >{group.category}</span
            >
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            <span class="font-medium">{fmt(gt.target)}</span>
            <span class="ml-0.5 text-[10px] font-normal text-muted-foreground">
              {viewMode === 'YEARLY' ? '/yr' : '/mo'}
            </span>
          </Table.Cell>
          <Table.Cell class="py-2.5 text-right tabular-nums">
            <span class="font-medium">{fmt(gt.target + gt.rollover)}</span>
          </Table.Cell>
          <Table.Cell class={cn('py-2.5 text-right tabular-nums', remainingColor(gt.remaining))}>
            {fmt(gt.spent)}
          </Table.Cell>
          <Table.Cell class="py-2.5">
            <span
              class={cn(
                'text-xs tabular-nums',
                gt.pct > 100 ? 'font-medium text-destructive' : 'text-muted-foreground'
              )}>{Math.max(0, gt.pct).toFixed(1)}%</span
            >
          </Table.Cell>
          <Table.Cell class={cn('py-2.5 text-right tabular-nums', remainingColor(gt.remaining))}>
            {gt.remaining >= 0 ? `+${fmt(gt.remaining)}` : fmt(gt.remaining)}
          </Table.Cell>
          <Table.Cell class="py-2.5"></Table.Cell>
        </Table.Row>
        {#each group.rows as item (item.budget.guid)}
          {@const actual = item.actual}
          <Table.Row>
            <Table.Cell class="py-2.5">
              <div class="flex items-center gap-2">
                <a
                  href={resolve('/accounts/[guid]', { guid: item.budget.accountGuid })}
                  class="font-medium hover:underline"
                >
                  {item.budget.accountDescription ?? item.budget.accountName}
                </a>
                {#if item.budget.notes}
                  <Tooltip.Root>
                    <Tooltip.Trigger>
                      {#snippet child({ props })}
                        <button
                          type="button"
                          {...props}
                          class="shrink-0 text-muted-foreground"
                          aria-label={`Notes: ${item.budget.notes}`}
                        >
                          <StickyNote aria-hidden="true" class="size-3" />
                        </button>
                      {/snippet}
                    </Tooltip.Trigger>
                    <Tooltip.Content>
                      <p class="max-w-64 text-xs whitespace-normal">{item.budget.notes}</p>
                    </Tooltip.Content>
                  </Tooltip.Root>
                {/if}
              </div>
            </Table.Cell>
            <Table.Cell class="py-2.5 text-right tabular-nums">
              {#if actual}
                <!-- Backend-effective base: already handles MONTHLY×12 and
                     FX conversion; don't recompute it on the client. -->
                {fmt(actual.budgetAmount)}
                <span class="ml-0.5 text-[10px] text-muted-foreground">
                  {viewMode === 'YEARLY' ? '/yr' : '/mo'}
                </span>
              {:else}
                <!-- No backend row yet: plain display fallback. -->
                {#if viewMode === 'YEARLY' && item.budget.periodType === 'MONTHLY'}
                  {fmt(item.budget.amount * 12)}
                  <span class="ml-0.5 text-[10px] text-muted-foreground">/yr</span>
                {:else}
                  {fmt(item.budget.amount)}
                  <span class="ml-0.5 text-[10px] text-muted-foreground">
                    {item.budget.periodType === 'YEARLY' ? '/yr' : '/mo'}
                  </span>
                {/if}
              {/if}
            </Table.Cell>
            <Table.Cell class="py-2.5 text-right tabular-nums">
              {#if actual}
                {fmt(Math.max(0, actual.budgetAmount + actual.rolloverBalance))}
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
              {actual ? fmt(actual.actualAmount) : '\u2014'}
            </Table.Cell>
            <Table.Cell class="py-2.5">
              {#if actual}
                <span
                  class={cn(
                    'text-xs tabular-nums',
                    actual.percentage > 100
                      ? 'font-medium text-destructive'
                      : 'text-muted-foreground'
                  )}>{Math.max(0, actual.percentage).toFixed(1)}%</span
                >
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
            <Table.Cell class="py-2.5">
              <div class="flex items-center justify-end gap-0.5">
                <Tooltip.Root>
                  <Tooltip.Trigger>
                    {#snippet child({ props })}
                      <Button
                        {...props}
                        variant="ghost"
                        size="icon-sm"
                        onclick={() => {
                          onEdit?.(item.budget);
                        }}
                        aria-label="Edit budget"
                      >
                        <Pencil class="size-3" />
                      </Button>
                    {/snippet}
                  </Tooltip.Trigger>
                  <Tooltip.Content>
                    <p>Edit budget</p>
                  </Tooltip.Content>
                </Tooltip.Root>
                <Tooltip.Root>
                  <Tooltip.Trigger>
                    {#snippet child({ props })}
                      <Button
                        {...props}
                        variant="ghost"
                        size="icon-sm"
                        class="text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                        onclick={() => {
                          onDeleteClick?.(item.budget.guid, item.budget.accountName);
                        }}
                        aria-label="Delete budget"
                      >
                        <Trash2 class="size-3" />
                      </Button>
                    {/snippet}
                  </Tooltip.Trigger>
                  <Tooltip.Content>
                    <p>Delete budget</p>
                  </Tooltip.Content>
                </Tooltip.Root>
              </div>
            </Table.Cell>
          </Table.Row>
        {/each}
      {/each}
    </Table.Body>
  </TableShell>
{/if}
