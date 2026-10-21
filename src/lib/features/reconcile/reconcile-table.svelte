<script lang="ts">
  import { tooltip } from '$lib/attachments/tooltip';
  import { resolve } from '$app/paths';
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import SortHead from '$lib/components/shared/sort-head.svelte';
  import EmptyValue from '$lib/components/shared/empty-value.svelte';
  import NavRow from '$lib/components/shared/nav-row.svelte';
  import { formatDate as fmtDate, formatSignedCurrency, signColor } from '$lib/utils/format.svelte';
  import { selectionState } from '$lib/features/reconcile/math';
  import type { ReconcileCandidate, TxSortKey } from '$lib/tauri';

  let {
    rows,
    checked,
    currency,
    duplicates,
    sortBy,
    sortDir,
    onSort,
    onToggle,
    onToggleAll,
    onRangeSelect
  }: {
    rows: ReconcileCandidate[];
    checked: Set<string>;
    currency: string;
    duplicates: Set<string>;
    sortBy: TxSortKey | null;
    sortDir: 'asc' | 'desc';
    onSort: (key: TxSortKey) => void;
    onToggle: (splitGuid: string, next: boolean) => void;
    onToggleAll: (next: boolean) => void;
    onRangeSelect: (splitGuid: string) => void;
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
      <Table.Head class="hidden text-muted-foreground @2xl:table-cell">Counter account</Table.Head>
      <Table.Head class="text-muted-foreground">Status</Table.Head>
      <SortHead
        label="Amount"
        active={sortBy === 'amount'}
        dir={sortDir}
        onclick={() => onSort('amount')}
        align="right"
      />
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {#each rows as row (row.splitGuid)}
      {@const isChecked = checked.has(row.splitGuid)}
      {@const isDuplicate = duplicates.has(row.splitGuid)}
      <NavRow
        onclickcapture={(e) => {
          // Capture before anything inside the row acts: shift-click marks a
          // contiguous range from the last selection, on any cell.
          if (e.shiftKey) {
            e.preventDefault();
            e.stopPropagation();
            onRangeSelect(row.splitGuid);
          }
        }}
        onNavigate={() => onToggle(row.splitGuid, !isChecked)}
      >
        <Table.Cell class="py-2.5" {@attach tooltip('Click to toggle · Shift-click for a range')}>
          <Checkbox
            checked={isChecked}
            aria-label="Mark {row.description ?? row.postDate} cleared"
            onCheckedChange={(v) => onToggle(row.splitGuid, v === true)}
          />
        </Table.Cell>
        <Table.Cell class="py-2.5 whitespace-nowrap text-muted-foreground tabular-nums">
          {fmtDate(row.postDate)}
        </Table.Cell>
        <Table.Cell class="max-w-md py-2.5 font-medium">
          <a
            href={resolve('/transactions/[guid]', { guid: row.txGuid })}
            {@attach tooltip('Open transaction', { truncated: true })}
            class="block truncate focus-visible:outline-2 focus-visible:outline-ring"
          >
            {#if row.num}<span class="mr-1 text-muted-foreground">#{row.num}</span>{/if}
            {#if row.description}
              {row.description}
            {:else}
              <EmptyValue />
            {/if}
          </a>
          {#if row.memo}
            <p
              class="truncate text-xs font-normal text-muted-foreground"
              {@attach tooltip(row.memo, { truncated: true })}
            >
              {row.memo}
            </p>
          {/if}
        </Table.Cell>
        <Table.Cell
          class="hidden max-w-40 truncate py-2.5 text-sm text-muted-foreground @2xl:table-cell"
        >
          {#if row.counterAccounts.length > 0}
            <span {@attach tooltip(row.counterAccounts.join(', '))}
              >{row.counterAccounts.join(', ')}</span
            >
          {:else}
            <EmptyValue />
          {/if}
        </Table.Cell>
        <Table.Cell class="py-2.5">
          <div class="flex flex-wrap items-center gap-1">
            <Badge variant={isChecked ? 'warning' : 'secondary'}>
              {isChecked ? 'Cleared' : 'New'}
            </Badge>
            {#if isDuplicate}
              <Badge
                variant="outline"
                {@attach tooltip(
                  'Shares a date and amount with another candidate — check for a duplicate import'
                )}
              >
                Duplicate?
              </Badge>
            {/if}
          </div>
        </Table.Cell>
        <Table.Cell
          class="py-2.5 text-right font-medium whitespace-nowrap tabular-nums {signColor(
            row.amount
          )}"
        >
          {formatSignedCurrency(row.amount, currency)}
        </Table.Cell>
      </NavRow>
    {/each}
  </Table.Body>
</TableShell>
