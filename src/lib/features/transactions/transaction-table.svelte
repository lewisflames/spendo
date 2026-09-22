<script lang="ts" module>
  export type TransactionColumnId =
    'date' | 'num' | 'accounts' | 'amount' | 'transfer' | 'status' | 'runningBalance';

  export const TRANSACTION_COLUMNS: { id: TransactionColumnId; label: string }[] = [
    { id: 'date', label: 'Date' },
    { id: 'num', label: 'Number' },
    { id: 'accounts', label: 'Accounts' },
    { id: 'amount', label: 'Total' }
  ];

  export const TRANSACTION_DEFAULT_COLUMNS: TransactionColumnId[] = ['date', 'amount'];
</script>

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

  let {
    transactions = [],
    currency = 'EUR',
    sortBy = null,
    sortDir = 'desc',
    onSort,
    variant = 'all',
    visibleColumns
  }: {
    transactions?: TransactionRow[] | DetailTransaction[];
    currency?: string;
    sortBy?: TxSortKey | null;
    sortDir?: 'asc' | 'desc';
    /** Direction handling lives with the caller's sort state; just report the key. */
    onSort?: (key: TxSortKey) => void;
    /** `account` adds Transfer/Status/Amount/Balance columns for the ledger view. */
    variant?: 'all' | 'account';
    /** Every column is visible when omitted. */
    visibleColumns?: TransactionColumnId[];
  } = $props();

  function isDetail(txn: TransactionRow | DetailTransaction): txn is DetailTransaction {
    return variant === 'account' && 'counterAccounts' in txn;
  }

  function isVisible(id: TransactionColumnId): boolean {
    return visibleColumns ? visibleColumns.includes(id) : true;
  }

  function totalOf(txn: TransactionRow | DetailTransaction): number {
    return isDetail(txn) ? txn.amount : (txn as TransactionRow).totalAmount;
  }

  // Date is fixed in the account ledger; only the all-transactions table can hide it.
  let showDate = $derived(variant !== 'all' || isVisible('date'));
  // Inline `#num` under the description is redundant once that column is shown.
  let showInlineNum = $derived(variant !== 'all' || !isVisible('num'));

  function splitAccounts(txn: TransactionRow | DetailTransaction): string[] {
    const out: string[] = [];
    for (const split of txn.splits) {
      if (!split.accountName || out.includes(split.accountName)) continue;
      out.push(split.accountName);
    }
    return out;
  }
</script>

{#snippet multiValue(values: string[])}
  {#if values.length === 0}
    {EMPTY_DASH}
  {:else}
    <div class="flex items-center gap-1">
      <span class="truncate">{values[0]}</span>
      {#if values.length > 1}
        <Badge variant="outline" class="shrink-0 text-[10px] leading-none"
          >+{values.length - 1} more</Badge
        >
      {/if}
    </div>
  {/if}
{/snippet}

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      {#if showDate}
        <SortHead
          label="Date"
          active={sortBy === 'date'}
          dir={sortDir}
          onclick={() => onSort?.('date')}
        />
      {/if}
      <SortHead
        label="Description"
        active={sortBy === 'description'}
        dir={sortDir}
        onclick={() => onSort?.('description')}
      />
      {#if variant === 'all'}
        {#if isVisible('num')}
          <Table.Head class="text-muted-foreground">Number</Table.Head>
        {/if}
        {#if isVisible('accounts')}
          <Table.Head class="text-muted-foreground">Accounts</Table.Head>
        {/if}
        {#if isVisible('amount')}
          <SortHead
            label="Total"
            active={sortBy === 'amount'}
            dir={sortDir}
            onclick={() => onSort?.('amount')}
            class="text-right"
          />
        {/if}
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
        {#if showDate}
          <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground">
            {fmtDate(txn.postDate)}
          </Table.Cell>
        {/if}
        <Table.Cell class="max-w-md py-2.5 font-medium">
          <!-- Accessible link target for keyboard/middle-click; the row click
               is a convenience shortcut. Its event must keep bubbling so
               SvelteKit's router can intercept it. -->
          <a
            class="block truncate focus-visible:outline-2 focus-visible:outline-ring"
            href={resolve('/transactions/[guid]', { guid: txn.guid })}
            title={txn.description ?? undefined}
          >
            {#if txn.num && showInlineNum}<span class="mr-1 text-muted-foreground">#{txn.num}</span
              >{/if}
            {txn.description || EMPTY_DASH}
          </a>
          {#if txn.notes}
            <p class="truncate text-xs font-normal text-muted-foreground" title={txn.notes}>
              {txn.notes}
            </p>
          {/if}
        </Table.Cell>
        {#if variant === 'all'}
          {#if isVisible('num')}
            <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground tabular-nums">
              {txn.num || EMPTY_DASH}
            </Table.Cell>
          {/if}
          {#if isVisible('accounts')}
            {@const accounts = splitAccounts(txn)}
            <Table.Cell class="max-w-xs py-2.5 text-muted-foreground" title={accounts.join(', ')}>
              {@render multiValue(accounts)}
            </Table.Cell>
          {/if}
          {#if isVisible('amount')}
            <Table.Cell
              class="py-2.5 text-right font-medium whitespace-nowrap tabular-nums {signColor(
                totalOf(txn)
              )}"
            >
              {formatSignedCurrency(totalOf(txn), currency)}
            </Table.Cell>
          {/if}
        {:else if isDetail(txn)}
          {#if isVisible('transfer')}
            <Table.Cell
              class="max-w-xs py-2.5 text-muted-foreground"
              title={txn.counterAccounts.join(', ')}
            >
              {@render multiValue(txn.counterAccounts)}
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
