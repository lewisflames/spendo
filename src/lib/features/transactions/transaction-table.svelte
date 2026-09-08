<script lang="ts">
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import {
    formatDate as fmtDate,
    formatSignedCurrency,
    formatSessionCurrency,
    signColor,
    EMPTY_DASH
  } from '$lib/utils/format.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import SortHead from '$lib/components/shared/sort-head.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import { openTransaction } from '$lib/utils/navigation';
  import { statusFor } from '$lib/utils/reconcile';
  import type { DetailTransaction, TransactionRow, TxSortKey } from '$lib/tauri';

  type AccountColumn = 'transfer' | 'status' | 'amount' | 'runningBalance';

  let {
    transactions = [],
    currency = 'EUR',
    sortBy = null,
    sortDir = 'desc',
    onSort,
    variant = 'all',
    visibleColumns = ['transfer', 'amount'] as AccountColumn[]
  }: {
    transactions?: TransactionRow[] | DetailTransaction[];
    currency?: string;
    sortBy?: TxSortKey | null;
    sortDir?: 'asc' | 'desc';
    /** Direction handling lives with the caller's sort state; just report the key. */
    onSort?: (key: TxSortKey) => void;
    /** `account` adds Transfer/Status/Amount/Balance columns for the ledger view. */
    variant?: 'all' | 'account';
    visibleColumns?: AccountColumn[];
  } = $props();

  function isDetail(txn: TransactionRow | DetailTransaction): txn is DetailTransaction {
    return variant === 'account' && 'counterAccounts' in txn;
  }

  function isVisible(id: AccountColumn): boolean {
    return visibleColumns.includes(id);
  }

  function totalOf(txn: TransactionRow | DetailTransaction): number {
    return isDetail(txn) ? txn.amount : (txn as TransactionRow).totalAmount;
  }
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <SortHead
        label="Date"
        active={sortBy === 'date'}
        dir={sortDir}
        onclick={() => onSort?.('date')}
      />
      <SortHead
        label="Description"
        active={sortBy === 'description'}
        dir={sortDir}
        onclick={() => onSort?.('description')}
      />
      {#if variant === 'all'}
        <SortHead
          label="Total"
          active={sortBy === 'amount'}
          dir={sortDir}
          onclick={() => onSort?.('amount')}
          class="text-right"
        />
      {:else}
        {#if isVisible('transfer')}
          <Table.Head class="text-muted-foreground">Transfer</Table.Head>
        {/if}
        {#if isVisible('status')}
          <Table.Head class="text-muted-foreground">Status</Table.Head>
        {/if}
        {#if isVisible('amount')}
          <SortHead
            label="Amount"
            active={sortBy === 'amount'}
            dir={sortDir}
            onclick={() => onSort?.('amount')}
            class="text-right"
          />
        {/if}
        {#if isVisible('runningBalance')}
          <Table.Head class="text-right text-muted-foreground">Balance</Table.Head>
        {/if}
      {/if}
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each transactions as txn (txn.guid)}
      <NavRow onNavigate={() => openTransaction(txn.guid)}>
        <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground">
          {fmtDate(txn.postDate)}
        </Table.Cell>
        <Table.Cell class="max-w-md py-2.5 font-medium">
          <!-- Accessible link target for keyboard/middle-click; the row click
               is a convenience shortcut. Its event must keep bubbling so
               SvelteKit's router can intercept it. -->
          <a
            class="block truncate"
            href={resolve('/transactions/[id]', { id: txn.guid })}
            title={txn.description ?? undefined}
          >
            {#if txn.num}<span class="mr-1 text-muted-foreground">#{txn.num}</span>{/if}
            {txn.description || EMPTY_DASH}
          </a>
          {#if txn.notes}
            <p class="truncate text-xs font-normal text-muted-foreground" title={txn.notes}>
              {txn.notes}
            </p>
          {/if}
        </Table.Cell>
        {#if variant === 'all'}
          <Table.Cell
            class="py-2.5 text-right font-medium whitespace-nowrap tabular-nums {signColor(
              totalOf(txn)
            )}"
          >
            {formatSignedCurrency(totalOf(txn), currency)}
          </Table.Cell>
        {:else if isDetail(txn)}
          {#if isVisible('transfer')}
            <Table.Cell
              class="max-w-xs py-2.5 text-muted-foreground"
              title={txn.counterAccounts.join(', ')}
            >
              {#if txn.counterAccounts.length === 0}
                {EMPTY_DASH}
              {:else}
                <div class="flex items-center gap-1">
                  <span class="truncate">{txn.counterAccounts[0]}</span>
                  {#if txn.counterAccounts.length > 1}
                    <Badge variant="outline" class="shrink-0 text-[10px] leading-none"
                      >+{txn.counterAccounts.length - 1} more</Badge
                    >
                  {/if}
                </div>
              {/if}
            </Table.Cell>
          {/if}
          {#if isVisible('status')}
            <Table.Cell class="py-2.5">
              {@const status = statusFor(txn.reconcileState)}
              <Badge variant={status.variant}>{status.label}</Badge>
            </Table.Cell>
          {/if}
          {#if isVisible('amount')}
            <Table.Cell
              class="py-2.5 text-right font-medium whitespace-nowrap tabular-nums {signColor(
                txn.amount
              )}"
            >
              {formatSignedCurrency(txn.amount, currency)}
            </Table.Cell>
          {/if}
          {#if isVisible('runningBalance')}
            <Table.Cell class="py-2.5 text-right font-medium whitespace-nowrap tabular-nums">
              {formatSessionCurrency(txn.runningBalance)}
            </Table.Cell>
          {/if}
        {/if}
      </NavRow>
    {/each}
  </Table.Body>
</TableShell>
