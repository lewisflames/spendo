<script lang="ts">
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import SortHead from '$lib/components/shared/sort-head.svelte';
  import { handleRowClick } from '$lib/utils/navigation';
  import {
    formatDate as fmtDate,
    formatSignedCurrency,
    signColor,
    EMPTY_DASH
  } from '$lib/utils/format.svelte';
  import { selectionState } from '$lib/features/reconcile/math';
  import type { ReconcileCandidate, TxSortKey } from '$lib/tauri';

  let {
    rows,
    checked,
    currency,
    sortBy,
    sortDir,
    onSort,
    onToggle,
    onToggleAll
  }: {
    rows: ReconcileCandidate[];
    checked: Set<string>;
    currency: string;
    sortBy: TxSortKey | null;
    sortDir: 'asc' | 'desc';
    onSort: (key: TxSortKey) => void;
    onToggle: (splitGuid: string, next: boolean) => void;
    onToggleAll: (next: boolean) => void;
  } = $props();

  let select = $derived(selectionState(checked, rows));
  let headerLabel = $derived(
    select === 'all'
      ? `Unmark all ${rows.length} filtered transactions`
      : `Mark all ${rows.length} filtered transactions cleared`
  );
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="w-10 text-muted-foreground">
        <Checkbox
          checked={select === 'all'}
          indeterminate={select === 'some'}
          disabled={rows.length === 0}
          aria-label={headerLabel}
          onCheckedChange={(v) => onToggleAll(v === true)}
        />
      </Table.Head>
      <SortHead
        label="Date"
        active={sortBy === 'date'}
        dir={sortDir}
        onclick={() => onSort('date')}
      />
      <SortHead
        label="Description"
        active={sortBy === 'description'}
        dir={sortDir}
        onclick={() => onSort('description')}
      />
      <Table.Head class="text-muted-foreground">Status</Table.Head>
      <SortHead
        label="Amount"
        active={sortBy === 'amount'}
        dir={sortDir}
        onclick={() => onSort('amount')}
        class="text-right"
      />
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each rows as row (row.splitGuid)}
      {@const isChecked = checked.has(row.splitGuid)}
      <Table.Row
        data-checked={isChecked}
        class="cursor-pointer hover:bg-muted/50"
        onclick={(e) => handleRowClick(e, () => onToggle(row.splitGuid, !isChecked))}
      >
        <Table.Cell class="py-2.5">
          <Checkbox
            checked={isChecked}
            aria-label="Mark {row.description ?? row.postDate} cleared"
            onCheckedChange={(v) => onToggle(row.splitGuid, v === true)}
          />
        </Table.Cell>
        <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground">
          {fmtDate(row.postDate)}
        </Table.Cell>
        <Table.Cell class="max-w-md py-2.5 font-medium">
          <a
            href={resolve('/transactions/[guid]', { guid: row.txGuid })}
            title="Open transaction"
            class="block truncate focus-visible:outline-2 focus-visible:outline-ring"
          >
            {#if row.num}<span class="mr-1 text-muted-foreground">#{row.num}</span>{/if}
            {row.description || EMPTY_DASH}
          </a>
          {#if row.memo}
            <p class="truncate text-xs font-normal text-muted-foreground" title={row.memo}>
              {row.memo}
            </p>
          {/if}
        </Table.Cell>
        <Table.Cell class="py-2.5">
          <Badge variant={isChecked ? 'warning' : 'secondary'}>
            {isChecked ? 'Cleared' : 'New'}
          </Badge>
        </Table.Cell>
        <Table.Cell
          class="py-2.5 text-right font-medium whitespace-nowrap tabular-nums {signColor(
            row.amount
          )}"
        >
          {formatSignedCurrency(row.amount, currency)}
        </Table.Cell>
      </Table.Row>
    {/each}
  </Table.Body>
</TableShell>
