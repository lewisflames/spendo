<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import TreeChevron from '$lib/components/shared/tree-chevron.svelte';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import { Upload } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings, saveSettings } from '$lib/stores/settings.svelte';
  import { fetchImportHistory } from '$lib/tauri';
  import type { ImportLogDetails } from '$lib/tauri';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { formatDate, formatNumber } from '$lib/utils/format.svelte';
  import ImportHistoryEntry from '$lib/features/home/import-history-entry.svelte';
  import { useTreeExpand } from '$lib/hooks/use-tree-expand.svelte';

  let page = $state(1);
  let pageSize = $derived(appSettings.value.pageSize);

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: async () => {
      let r = await fetchImportHistory(page, pageSize);
      // The backend clamps out-of-range pages; snap back to the last valid
      // page (possible after shrinking pageSize).
      if (page > r.totalPages) {
        page = r.totalPages;
        r = await fetchImportHistory(page, pageSize);
      }
      return r;
    },
    label: 'Failed to load import history'
  });

  let loading = $derived(res.loading);
  let data = $derived(res.data);
  let error = $derived(res.error);

  const tree = useTreeExpand<number>();

  function handlePageSizeChange(size: number) {
    saveSettings({ pageSize: size });
    page = 1;
  }

  function totalAdded(entry: ImportLogDetails): number {
    return (
      entry.accountsAdded + entry.transactionsAdded + entry.splitsAdded + entry.commoditiesAdded
    );
  }

  function totalUpdated(entry: ImportLogDetails): number {
    return entry.accountsUpdated + entry.transactionsUpdated + entry.splitsUpdated;
  }

  function totalOrphaned(entry: ImportLogDetails): number {
    return (
      entry.accountsOrphaned +
      entry.transactionsOrphaned +
      entry.splitsOrphaned +
      entry.commoditiesOrphaned
    );
  }
</script>

{#if loading && !data}
  <TableSkeleton cols={5} />
{:else if error}
  <ErrorCard message={error} onRetry={() => res.reload()} />
{:else if data}
  {#if data.logs.length === 0}
    <EmptyState
      card
      title="No imports yet"
      description="Import a .gnucash file to start analyzing your finances."
    >
      {#snippet icon()}
        <Upload aria-hidden="true" class="text-muted-foreground" />
      {/snippet}
    </EmptyState>
  {:else}
    <!-- Keep the previous table visible (dimmed) during refetches so the
           page doesn't jump and pagination stays in place. -->
    <DimWrapper active={loading} class="flex flex-col gap-4">
      <TableShell>
        <Table.Header>
          <Table.Row class="bg-muted/75">
            <Table.Head class="w-8"></Table.Head>
            <Table.Head>Date</Table.Head>
            <Table.Head class="text-right">Added</Table.Head>
            <Table.Head class="text-right">Updated</Table.Head>
            <Table.Head class="text-right">Orphaned</Table.Head>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {#each data.logs as entry (entry.id)}
            <Table.Row
              class="cursor-pointer hover:bg-muted/50"
              onclick={() => tree.toggle(entry.id)}
            >
              <Table.Cell class="py-2.5">
                <TreeChevron
                  open={tree.expanded.has(entry.id)}
                  ontoggle={() => tree.toggle(entry.id)}
                />
              </Table.Cell>
              <Table.Cell class="py-2.5 whitespace-nowrap">
                <span class="tabular-nums">
                  {formatDate(entry.importedAt, {
                    month: 'short',
                    day: 'numeric',
                    year: 'numeric',
                    hour: 'numeric',
                    minute: '2-digit'
                  })}
                </span>
                <span
                  class="block max-w-96 truncate text-xs text-muted-foreground/70"
                  title={entry.filePath ?? entry.filename}
                >
                  {entry.filePath ?? entry.filename}
                </span>
              </Table.Cell>
              <Table.Cell
                class="py-2.5 text-right whitespace-nowrap tabular-nums {totalAdded(entry) === 0
                  ? 'text-muted-foreground'
                  : ''}"
              >
                {totalAdded(entry) === 0 ? '\u2014' : formatNumber(totalAdded(entry))}
              </Table.Cell>
              <Table.Cell
                class="py-2.5 text-right whitespace-nowrap tabular-nums {totalUpdated(entry) === 0
                  ? 'text-muted-foreground'
                  : ''}"
              >
                {totalUpdated(entry) === 0 ? '\u2014' : formatNumber(totalUpdated(entry))}
              </Table.Cell>
              <Table.Cell
                class="py-2.5 text-right whitespace-nowrap tabular-nums {totalOrphaned(entry) === 0
                  ? 'text-muted-foreground'
                  : ''}"
              >
                {totalOrphaned(entry) === 0 ? '\u2014' : formatNumber(totalOrphaned(entry))}
              </Table.Cell>
            </Table.Row>
            {#if tree.expanded.has(entry.id)}
              <ImportHistoryEntry {entry} />
            {/if}
          {/each}
        </Table.Body>
      </TableShell>
      <PaginationNav
        bind:page
        totalPages={data.totalPages}
        totalItems={data.total}
        pageSize={appSettings.value.pageSize}
        onPageSizeChange={handlePageSizeChange}
      />
    </DimWrapper>
  {/if}
{/if}
