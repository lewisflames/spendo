<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import SortHead from '$lib/components/shared/sort-head.svelte';
  import { statusFor } from '$lib/utils/reconcile';
  import {
    formatDate as fmtDate,
    formatSignedCurrency,
    signColor,
    EMPTY_DASH
  } from '$lib/utils/format.svelte';
  import type { ReconcileCandidate, TxSortKey } from '$lib/tauri';

  let {
    rows,
    checked,
    currency,
    sortBy,
    sortDir,
    onSort,
    onToggle
  }: {
    rows: ReconcileCandidate[];
    checked: Set<string>;
    currency: string;
    sortBy: TxSortKey | null;
    sortDir: 'asc' | 'desc';
    onSort: (key: TxSortKey) => void;
    onToggle: (splitGuid: string, next: boolean) => void;
  } = $props();
</script>

<TableShell>
  <Table.Header>
    <Table.Row class="bg-muted/75">
      <Table.Head class="w-10 text-muted-foreground">
        <span class="sr-only">Cleared</span>
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
      {@const status = statusFor(row.reconcileState)}
      {@const isChecked = checked.has(row.splitGuid)}
      <Table.Row data-checked={isChecked}>
        <Table.Cell class="py-2.5">
          <input
            type="checkbox"
            class="size-4 accent-primary"
            checked={isChecked}
            aria-label="Mark {row.description ?? row.postDate} cleared"
            onchange={(e) => onToggle(row.splitGuid, e.currentTarget.checked)}
          />
        </Table.Cell>
        <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground">
          {fmtDate(row.postDate)}
        </Table.Cell>
        <Table.Cell class="max-w-md py-2.5 font-medium">
          <span class="block truncate" title={row.description ?? undefined}>
            {#if row.num}<span class="mr-1 text-muted-foreground">#{row.num}</span>{/if}
            {row.description || EMPTY_DASH}
          </span>
          {#if row.memo}
            <p class="truncate text-xs font-normal text-muted-foreground" title={row.memo}>
              {row.memo}
            </p>
          {/if}
        </Table.Cell>
        <Table.Cell class="py-2.5">
          <Badge variant={isChecked ? 'warning' : status.variant}>
            {isChecked ? 'Cleared' : status.label}
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
