<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import type { ImportLogDetails } from '$lib/tauri';

  let { entry }: { entry: ImportLogDetails } = $props();

  const kinds = ['added', 'updated', 'orphaned'] as const;

  const rows = $derived([
    {
      label: 'Accounts',
      added: entry.accountsAdded,
      updated: entry.accountsUpdated,
      orphaned: entry.accountsOrphaned
    },
    {
      label: 'Transactions',
      added: entry.transactionsAdded,
      updated: entry.transactionsUpdated,
      orphaned: entry.transactionsOrphaned
    },
    {
      label: 'Splits',
      added: entry.splitsAdded,
      updated: entry.splitsUpdated,
      orphaned: entry.splitsOrphaned
    },
    {
      label: 'Commodities',
      added: entry.commoditiesAdded,
      updated: null,
      orphaned: entry.commoditiesOrphaned
    }
  ]);
</script>

{#each rows as row (row.label)}
  <Table.Row class="bg-muted/25 hover:bg-muted/25">
    <Table.Cell class="py-1.5" aria-hidden="true"></Table.Cell>
    <Table.Cell class="py-1.5 text-xs text-muted-foreground">{row.label}</Table.Cell>
    {#each kinds as kind (kind)}
      {@const value = row[kind]}
      <Table.Cell
        class="py-1.5 text-right text-xs tabular-nums {value === null || value === 0
          ? 'text-muted-foreground'
          : kind === 'added'
            ? 'text-success'
            : kind === 'updated'
              ? 'text-warning'
              : 'text-destructive'}"
      >
        {value === null || value === 0 ? '\u2014' : value}
      </Table.Cell>
    {/each}
  </Table.Row>
{/each}
