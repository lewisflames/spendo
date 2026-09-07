<script lang="ts" module>
  export type ActivityColumnId =
    'action' | 'account' | 'shares' | 'price' | 'value' | 'runningBalance';

  export const ACTIVITY_COLUMNS: { id: ActivityColumnId; label: string }[] = [
    { id: 'action', label: 'Action' },
    { id: 'account', label: 'Account' },
    { id: 'shares', label: 'Shares' },
    { id: 'price', label: 'Price' },
    { id: 'value', label: 'Value' },
    { id: 'runningBalance', label: 'Balance' }
  ];

  export const ACTIVITY_DEFAULT_COLUMNS: ActivityColumnId[] = ['action', 'shares', 'value'];
</script>

<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { resolve } from '$app/paths';
  import { Receipt } from '@lucide/svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import { appSettings } from '$lib/stores/settings.svelte';
  import type { ColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import {
    signColor,
    formatSignedCurrency,
    formatCurrency,
    formatDate,
    EMPTY_DASH
  } from '$lib/utils/format.svelte';
  import { handleRowClick, openTransaction } from '$lib/utils/navigation';
  import type { HoldingTransaction } from '$lib/tauri';

  let {
    transactions,
    currency,
    cols,
    loading = false,
    page = $bindable(1),
    total = 0,
    totalPages = 1,
    onPageSizeChange
  }: {
    transactions: HoldingTransaction[];
    currency: string;
    cols: ColumnVisibility<ActivityColumnId>;
    loading?: boolean;
    page: number;
    total: number;
    totalPages: number;
    onPageSizeChange: (size: number) => void;
  } = $props();

  function isVisible(id: ActivityColumnId): boolean {
    return cols.isVisible(id);
  }

  /** Buy/Sell derived from the share-delta sign; near-zero rows have no action. */
  function actionFor(shares: number): { label: string; variant: 'success' | 'destructive' } | null {
    if (shares > 1e-9) return { label: 'Buy', variant: 'success' };
    if (shares < -1e-9) return { label: 'Sell', variant: 'destructive' };
    return null;
  }

  /** Per-share price; null when no shares moved (avoids divide-by-zero). */
  function pricePerShare(t: HoldingTransaction): number | null {
    if (Math.abs(t.sharesDelta) < 1e-9) return null;
    return t.valueDelta / t.sharesDelta;
  }
</script>

<!-- Buy/Sell activity -->
{#if total === 0}
  <EmptyState card title="No activity" description="No buys or sells found for this security.">
    {#snippet icon()}
      <Receipt aria-hidden="true" class="text-muted-foreground" />
    {/snippet}
  </EmptyState>
{:else}
  <!-- Keep the previous page visible (dimmed) during refetches so the
       page doesn't jump and pagination stays in place. -->
  <DimWrapper active={loading} class="flex flex-col gap-4">
    <TableShell>
      <Table.Header>
        <Table.Row class="bg-muted/75">
          <Table.Head class="text-muted-foreground">Date</Table.Head>
          <Table.Head class="text-muted-foreground">Description</Table.Head>
          {#if isVisible('action')}
            <Table.Head class="text-muted-foreground">Action</Table.Head>
          {/if}
          {#if isVisible('account')}
            <Table.Head class="text-muted-foreground">Account</Table.Head>
          {/if}
          {#if isVisible('shares')}
            <Table.Head class="text-right text-muted-foreground">Shares</Table.Head>
          {/if}
          {#if isVisible('price')}
            <Table.Head class="text-right text-muted-foreground">Price</Table.Head>
          {/if}
          {#if isVisible('value')}
            <Table.Head class="text-right text-muted-foreground">Value</Table.Head>
          {/if}
          {#if isVisible('runningBalance')}
            <Table.Head class="text-right text-muted-foreground">Balance</Table.Head>
          {/if}
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each transactions as t (t.guid)}
          <Table.Row
            class="cursor-pointer hover:bg-muted/50"
            onclick={(e) => handleRowClick(e, () => openTransaction(t.guid))}
          >
            <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground tabular-nums">
              {formatDate(t.postDate)}
            </Table.Cell>
            <Table.Cell class="max-w-md py-2.5 font-medium">
              <!-- Accessible link target for keyboard/middle-click; the row click
                     is a convenience shortcut. Its event must keep bubbling so
                     SvelteKit's router can intercept it. -->
              <a
                class="block truncate"
                href={resolve('/transactions/[id]', { id: t.guid })}
                title={t.description ?? undefined}
              >
                {#if t.num}<span class="mr-1 text-muted-foreground">#{t.num}</span>{/if}
                {t.description || EMPTY_DASH}
              </a>
              {#if t.notes}
                <p class="truncate text-xs font-normal text-muted-foreground" title={t.notes}>
                  {t.notes}
                </p>
              {/if}
            </Table.Cell>
            {#if isVisible('action')}
              <Table.Cell class="py-2.5">
                {@const action = actionFor(t.sharesDelta)}
                {#if action}
                  <Badge variant={action.variant}>{action.label}</Badge>
                {:else}
                  {EMPTY_DASH}
                {/if}
              </Table.Cell>
            {/if}
            {#if isVisible('account')}
              <Table.Cell class="max-w-48 truncate py-2.5 text-xs text-muted-foreground">
                {t.accountDescription || t.accountName}
              </Table.Cell>
            {/if}
            {#if isVisible('shares')}
              <Table.Cell class="py-2.5 text-right tabular-nums {signColor(t.sharesDelta)}">
                {t.sharesDelta >= 0 ? '+' : ''}{t.sharesDelta.toFixed(2)}
              </Table.Cell>
            {/if}
            {#if isVisible('price')}
              {@const unitPrice = pricePerShare(t)}
              <Table.Cell class="py-2.5 text-right text-muted-foreground tabular-nums">
                {unitPrice === null ? EMPTY_DASH : formatCurrency(unitPrice, currency)}
              </Table.Cell>
            {/if}
            {#if isVisible('value')}
              <Table.Cell class="py-2.5 text-right tabular-nums {signColor(t.valueDelta)}">
                {formatSignedCurrency(t.valueDelta, currency)}
              </Table.Cell>
            {/if}
            {#if isVisible('runningBalance')}
              <Table.Cell
                class="py-2.5 text-right text-muted-foreground tabular-nums"
                title="Share balance after this transaction"
              >
                {t.runningShares.toFixed(2)}
              </Table.Cell>
            {/if}
          </Table.Row>
        {/each}
      </Table.Body>
    </TableShell>
    <PaginationNav
      bind:page
      {totalPages}
      totalItems={total}
      pageSize={appSettings.value.pageSize}
      {onPageSizeChange}
    />
  </DimWrapper>
{/if}
