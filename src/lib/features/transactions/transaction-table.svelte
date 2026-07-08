<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { formatDate as fmtDate, formatSignedCurrency, signColor } from '$lib/utils/format.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import SortHead from '$lib/components/shared/sort-head.svelte';
  import type { TransactionRow } from '$lib/tauri';

  let {
    transactions = [],
    currency = 'EUR',
    sortBy = null,
    sortDir = 'desc',
    onSort
  }: {
    transactions?: TransactionRow[];
    currency?: string;
    sortBy?: 'date' | 'description' | 'amount' | null;
    sortDir?: 'asc' | 'desc';
    onSort?: (key: 'date' | 'description' | 'amount', dir: 'asc' | 'desc') => void;
  } = $props();

  function toggleSort(key: 'date' | 'description' | 'amount') {
    if (sortBy === key) {
      onSort?.(key, sortDir === 'asc' ? 'desc' : 'asc');
    } else {
      onSort?.(key, key === 'description' ? 'asc' : 'desc');
    }
  }

  function openTransaction(id: string) {
    void goto(resolve('/transactions/[id]', { id }));
  }

  /**
   * Row-level navigation shortcut for plain clicks anywhere on the row.
   * Clicks on real links bubble up untouched: SvelteKit's root click listener
   * performs their client-side navigation (stopPropagation on the link would
   * defeat that interceptor and cause a full page reload).
   */
  function handleRowClick(e: MouseEvent, id: string) {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) {
      return;
    }
    if ((e.target as HTMLElement).closest('a')) return;
    openTransaction(id);
  }
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <SortHead
        label="Date"
        active={sortBy === 'date'}
        dir={sortDir}
        onclick={() => toggleSort('date')}
      />
      <SortHead
        label="Description"
        active={sortBy === 'description'}
        dir={sortDir}
        onclick={() => toggleSort('description')}
      />
      <SortHead
        label="Total"
        active={sortBy === 'amount'}
        dir={sortDir}
        onclick={() => toggleSort('amount')}
        class="text-right"
      />
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each transactions as txn (txn.guid)}
      <Table.Row
        class="cursor-pointer hover:bg-muted/50"
        onclick={(e) => handleRowClick(e, txn.guid)}
      >
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
            {txn.description || '\u2014'}
          </a>
          {#if txn.notes}
            <p class="truncate text-xs font-normal text-muted-foreground" title={txn.notes}>
              {txn.notes}
            </p>
          {/if}
        </Table.Cell>
        <Table.Cell
          class="py-2.5 text-right font-medium whitespace-nowrap tabular-nums {signColor(
            txn.totalAmount
          )}"
        >
          {formatSignedCurrency(txn.totalAmount, currency)}
        </Table.Cell>
      </Table.Row>
    {/each}
  </Table.Body>
</TableShell>
