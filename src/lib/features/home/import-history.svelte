<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import { Upload } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchImportHistory } from '$lib/tauri';
  import { formatDate } from '$lib/utils/format.svelte';
  import type { ImportLogDetails } from '$lib/tauri';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';

  const res = useAsyncData<ImportLogDetails[]>({
    enabled: () => !!dbSession.value,
    fetch: () => fetchImportHistory(),
    label: 'Failed to load import history'
  });

  let importHistory = $derived(res.data);
  let loading = $derived(res.loading);
  let error = $derived(res.error);
</script>

{#if loading}
  <TableSkeleton cols={6} rows={4} />
{:else if error}
  <ErrorCard message={error} />
{:else if importHistory === null || importHistory.length === 0}
  <EmptyState
    title="No imports yet"
    description="Import a .gnucash file to start analyzing your finances."
  >
    {#snippet icon()}
      <Upload aria-hidden="true" class="text-muted-foreground" />
    {/snippet}
  </EmptyState>
{:else}
  {#snippet deltaCell(added: number, updated: number, removed: number)}
    {#if added > 0 || updated > 0 || removed > 0}
      <span class="flex items-center gap-1.5 text-xs tabular-nums">
        {#if added > 0}
          <span class="text-success">+{added}</span>
        {/if}
        {#if updated > 0}
          <span class="text-warning">~{updated}</span>
        {/if}
        {#if removed > 0}
          <span
            title="Records that were in the database but not in the imported file"
            class="text-destructive">-{removed}</span
          >
        {/if}
      </span>
    {:else}
      <span class="text-muted-foreground">—</span>
    {/if}
  {/snippet}

  <TableShell>
    <Table.Header>
      <Table.Row class="bg-muted/75">
        <Table.Head class="text-muted-foreground">File</Table.Head>
        <Table.Head class="text-muted-foreground">Date</Table.Head>
        <Table.Head class="text-muted-foreground">Accounts</Table.Head>
        <Table.Head class="text-muted-foreground">Transactions</Table.Head>
        <Table.Head class="text-muted-foreground">Splits</Table.Head>
        <Table.Head class="text-muted-foreground">Commodities</Table.Head>
      </Table.Row>
    </Table.Header>
    <Table.Body>
      {#each importHistory as entry (entry.id)}
        <Table.Row>
          <Table.Cell
            class="max-w-56 truncate py-2 font-medium"
            title={entry.filePath ?? entry.filename}
          >
            {entry.filename}
          </Table.Cell>
          <Table.Cell class="py-2 whitespace-nowrap text-muted-foreground">
            {formatDate(entry.importedAt, {
              month: 'short',
              day: 'numeric',
              year: 'numeric',
              hour: 'numeric',
              minute: '2-digit'
            })}
          </Table.Cell>
          <Table.Cell class="py-2">
            {@render deltaCell(entry.accountsAdded, entry.accountsUpdated, entry.accountsOrphaned)}
          </Table.Cell>
          <Table.Cell class="py-2">
            {@render deltaCell(
              entry.transactionsAdded,
              entry.transactionsUpdated,
              entry.transactionsOrphaned
            )}
          </Table.Cell>
          <Table.Cell class="py-2">
            {@render deltaCell(entry.splitsAdded, entry.splitsUpdated, entry.splitsOrphaned)}
          </Table.Cell>
          <Table.Cell class="py-2">
            {@render deltaCell(entry.commoditiesAdded, 0, entry.commoditiesOrphaned)}
          </Table.Cell>
        </Table.Row>
      {/each}
    </Table.Body>
  </TableShell>
{/if}
