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
  import Amount from '$lib/components/shared/amount.svelte';
  import { tooltip } from '$lib/attachments/tooltip';
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { formatDate as fmtDate, formatSignedCurrency } from '$lib/utils/format.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import SortHead from '$lib/components/shared/sort-head.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import { openTransaction } from '$lib/utils/navigation';
  import { statusFor } from '$lib/utils/reconcile';
  import TagBadges from '$lib/features/tags/tag-badges.svelte';
  import { pageSelectionState } from './bulk-edit';
  import type { DetailTransaction, TransactionRow, TxSortKey } from '$lib/tauri';

  let {
    transactions = [],
    currency = 'EUR',
    sortBy = null,
    sortDir = 'desc',
    onSort,
    variant = 'all',
    visibleColumns,
    selected = null,
    onToggle,
    onToggleAll,
    onRangeSelect
  }: {
    transactions?: TransactionRow[] | DetailTransaction[];
    currency?: string;
    sortBy?: TxSortKey | null;
    sortDir?: 'asc' | 'desc';
    /** Direction handling lives with the caller’s sort state; just report the key. */
    onSort?: (key: TxSortKey) => void;
    /** `account` adds Transfer/Status/Amount/Balance columns for the ledger view. */
    variant?: 'all' | 'account';
    /** Every column is visible when omitted. */
    visibleColumns?: TransactionColumnId[];
    /** Bulk-edit selection (all-transactions table); adds a checkbox column when set. */
    selected?: Pick<Map<string, unknown>, 'has'> | null;
    onToggle?: (txn: TransactionRow, next: boolean) => void;
    onToggleAll?: (next: boolean) => void;
    /** Shift-click on any cell: select from the last toggled row to this one. */
    onRangeSelect?: (guid: string) => void;
  } = $props();

  let selectable = $derived(variant === 'all' && !!selected);
  let headerState = $derived(selected ? pageSelectionState(selected, transactions) : 'none');

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
    <EmptyValue />
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
      {#if selectable}
        <Table.Head class="w-10 text-muted-foreground">
          <Checkbox
            checked={headerState === 'all'}
            indeterminate={headerState === 'some'}
            aria-label={headerState === 'all'
              ? 'Deselect the transactions on this page'
              : 'Select the transactions on this page'}
            onCheckedChange={(v) => onToggleAll?.(v === true)}
          />
        </Table.Head>
      {/if}
      {#if showDate}
        <SortHead
          label="Date"
          active={sortBy === 'date'}
          dir={sortDir}
          onclick={onSort ? () => onSort('date') : undefined}
        />
      {/if}
      <SortHead
        label="Description"
        active={sortBy === 'description'}
        dir={sortDir}
        onclick={onSort ? () => onSort('description') : undefined}
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
            onclick={onSort ? () => onSort('amount') : undefined}
            align="right"
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
            onclick={onSort ? () => onSort('amount') : undefined}
            align="right"
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
      {@const isSelected = selectable && !!selected?.has(txn.guid)}
      <NavRow
        onNavigate={() => openTransaction(txn.guid)}
        onclickcapture={selectable
          ? (e) => {
              // Capture before the row navigates or the checkbox toggles:
              // shift-click selects a contiguous range, on any cell.
              if (!e.shiftKey) return;
              e.preventDefault();
              e.stopPropagation();
              onRangeSelect?.(txn.guid);
            }
          : null}
        class={isSelected ? 'cursor-pointer bg-muted/40 hover:bg-muted/60' : undefined}
      >
        {#if selectable}
          <!-- The label fills the cell so a near-miss toggles instead of
               opening the row (the row-click guard ignores labels). -->
          <Table.Cell class="w-10 py-0">
            <label
              class="flex cursor-pointer items-center py-2.5"
              {@attach tooltip('Shift-click for a range')}
            >
              <Checkbox
                checked={isSelected}
                aria-label="Select {txn.description ?? fmtDate(txn.postDate)}"
                onCheckedChange={(v) => onToggle?.(txn as TransactionRow, v === true)}
              />
            </label>
          </Table.Cell>
        {/if}
        {#if showDate}
          <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground tabular-nums">
            {fmtDate(txn.postDate)}
          </Table.Cell>
        {/if}
        <Table.Cell class="max-w-md py-2.5 font-medium">
          <div class="min-w-0">
            <!-- Accessible link target for keyboard/middle-click; the row click
                   is a convenience shortcut. Its event must keep bubbling so
                   SvelteKit’s router can intercept it. -->
            <a
              class="block truncate focus-visible:outline-2 focus-visible:outline-ring"
              href={resolve('/transactions/[guid]', { guid: txn.guid })}
              {@attach tooltip(txn.description ?? undefined, { truncated: true })}
            >
              {#if txn.num && showInlineNum}<span class="mr-1 text-muted-foreground"
                  >#{txn.num}</span
                >{/if}
              {#if txn.description}
                {txn.description}
              {:else}
                <EmptyValue />
              {/if}
            </a>
            {#if txn.notes}
              <p
                class="truncate text-xs font-normal text-muted-foreground"
                {@attach tooltip(txn.notes, { truncated: true })}
              >
                {txn.notes}
              </p>
            {/if}
            {#if 'tags' in txn && txn.tags.length > 0}
              <TagBadges tags={txn.tags} class="mt-1" />
            {/if}
          </div>
        </Table.Cell>
        {#if variant === 'all'}
          {#if isVisible('num')}
            <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground tabular-nums">
              {#if txn.num}
                {txn.num}
              {:else}
                <EmptyValue />
              {/if}
            </Table.Cell>
          {/if}
          {#if isVisible('accounts')}
            {@const accounts = splitAccounts(txn)}
            <Table.Cell
              class="max-w-xs py-2.5 text-muted-foreground"
              {@attach tooltip(accounts.join(', '))}
            >
              {@render multiValue(accounts)}
            </Table.Cell>
          {/if}
          {#if isVisible('amount')}
            <Table.Cell class="py-2.5 text-right">
              <Amount value={totalOf(txn)} format={(n) => formatSignedCurrency(n, currency)} tone />
            </Table.Cell>
          {/if}
        {:else if isDetail(txn)}
          {#if isVisible('transfer')}
            <Table.Cell
              class="max-w-xs py-2.5 text-muted-foreground"
              {@attach tooltip(txn.counterAccounts.join(', '))}
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
            <Table.Cell class="py-2.5 text-right">
              <Amount value={txn.amount} format={(n) => formatSignedCurrency(n, currency)} tone />
            </Table.Cell>
          {/if}
          {#if isVisible('runningBalance')}
            <Table.Cell class="py-2.5 text-right">
              <Amount value={txn.runningBalance} />
            </Table.Cell>
          {/if}
        {/if}
      </NavRow>
    {/each}
  </Table.Body>
</TableShell>
