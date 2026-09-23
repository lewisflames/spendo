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
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import { appSettings } from '$lib/stores/settings.svelte';
  import type { ColumnVisibility } from '$lib/hooks/use-column-visibility.svelte';
  import {
    signColor,
    formatSignedCurrency,
    formatCurrency,
    formatDate,
    formatShares,
    EMPTY_DASH
  } from '$lib/utils/format.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import { openTransaction } from '$lib/utils/navigation';
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
          {#if cols.isVisible('action')}
            <Table.Head class="text-muted-foreground">Action</Table.Head>
          {/if}
          {#if cols.isVisible('account')}
            <Table.Head class="text-muted-foreground">Account</Table.Head>
          {/if}
          {#if cols.isVisible('shares')}
            <Table.Head class="text-right text-muted-foreground">Shares</Table.Head>
          {/if}
          {#if cols.isVisible('price')}
            <Table.Head class="text-right text-muted-foreground">Price</Table.Head>
          {/if}
          {#if cols.isVisible('value')}
            <Table.Head class="text-right text-muted-foreground">Value</Table.Head>
          {/if}
          {#if cols.isVisible('runningBalance')}
            <Table.Head class="text-right text-muted-foreground">Balance</Table.Head>
          {/if}
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each transactions as t (t.guid)}
          <NavRow onNavigate={() => openTransaction(t.guid)}>
            <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground tabular-nums">
              {formatDate(t.postDate)}
            </Table.Cell>
            <Table.Cell class="max-w-md py-2.5 font-medium">
              <!-- Accessible link target for keyboard/middle-click; the row click
                     is a convenience shortcut. Its event must keep bubbling so
                     SvelteKit's router can intercept it. -->
              <a
                class="block truncate focus-visible:outline-2 focus-visible:outline-ring"
                href={resolve('/transactions/[guid]', { guid: t.guid })}
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
            {#if cols.isVisible('action')}
              <Table.Cell class="py-2.5">
                {@const action = actionFor(t.sharesDelta)}
                {#if action}
                  <Badge variant={action.variant}>{action.label}</Badge>
                {:else}
                  <EmptyValue />
                {/if}
              </Table.Cell>
            {/if}
            {#if cols.isVisible('account')}
              <Table.Cell class="max-w-48 truncate py-2.5 text-xs text-muted-foreground">
                {t.accountDescription || t.accountName}
              </Table.Cell>
            {/if}
            {#if cols.isVisible('shares')}
              <Table.Cell class="py-2.5 text-right tabular-nums {signColor(t.sharesDelta)}">
                {t.sharesDelta >= 0 ? '+' : ''}{formatShares(Math.abs(t.sharesDelta))}
              </Table.Cell>
            {/if}
            {#if cols.isVisible('price')}
              {@const unitPrice = pricePerShare(t)}
              <Table.Cell class="py-2.5 text-right text-muted-foreground tabular-nums">
                {#if unitPrice === null}
                  <EmptyValue />
                {:else}
                  {formatCurrency(unitPrice, currency)}
                {/if}
              </Table.Cell>
            {/if}
            {#if cols.isVisible('value')}
              <Table.Cell class="py-2.5 text-right tabular-nums {signColor(t.valueDelta)}">
                {formatSignedCurrency(t.valueDelta, currency)}
              </Table.Cell>
            {/if}
            {#if cols.isVisible('runningBalance')}
              <Table.Cell
                class="py-2.5 text-right text-muted-foreground tabular-nums"
                title="Share balance after this transaction"
              >
                {formatShares(t.runningShares)}
              </Table.Cell>
            {/if}
          </NavRow>
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
