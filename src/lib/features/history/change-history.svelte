<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import TableShell from '$lib/components/shared/table-shell.svelte';
  import TableSkeleton from '$lib/components/shared/table-skeleton.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import { PaginationNav } from '$lib/components/shared/pagination-nav';
  import { History, Undo2, LoaderCircle } from '@lucide/svelte';
  import { dbSession, setDbSession } from '$lib/stores/db.svelte';
  import { appSettings, applyPageSize } from '$lib/stores/settings.svelte';
  import { fetchChangeHistory, revertHistoryEntry } from '$lib/tauri';
  import type { ChangeEntry } from '$lib/tauri';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { formatDate, plural, EMPTY_DASH } from '$lib/utils/format.svelte';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';
  import { impactLabel, sourceLabel } from './history-labels';

  let page = $state(1);
  let pageSize = $derived(appSettings.value.pageSize);

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: async () => {
      // Subscribe to summary changes so imports and reverts refresh
      // this list without navigation (the effect tracks these reads).
      void dbSession.value?.summary.totalTransactions;
      void dbSession.value?.summary.totalAccounts;
      let r = await fetchChangeHistory(page, pageSize);
      // The backend clamps out-of-range pages; snap back to the last valid
      // page (possible after shrinking pageSize).
      if (page > r.totalPages) {
        page = r.totalPages;
        r = await fetchChangeHistory(page, pageSize);
      }
      return r;
    },
    label: 'Failed to load changes history'
  });

  let confirmOpen = $state(false);
  let pending = $state<ChangeEntry | null>(null);
  let reverting = $state(false);

  function openConfirm(entry: ChangeEntry) {
    pending = entry;
    confirmOpen = true;
  }

  function closeConfirm() {
    confirmOpen = false;
    pending = null;
  }

  async function handleRevert() {
    if (!pending || reverting) return;
    reverting = true;
    try {
      const result = await revertHistoryEntry(pending.id);
      setDbSession(result.summary);
      toast.success('Change reverted');
      closeConfirm();
      res.reload();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Revert failed'));
    } finally {
      reverting = false;
    }
  }
</script>

{#if res.loading && !res.data}
  <TableSkeleton cols={4} />
{:else if res.data}
  {#if res.error}
    <ErrorCard message={res.error} onRetry={() => res.reload()} />
  {/if}
  {#if res.data.entries.length === 0}
    <EmptyState
      card
      title="No changes yet"
      description="Imports, edits and reverts will appear here."
    >
      {#snippet icon()}
        <History aria-hidden="true" class="text-muted-foreground" />
      {/snippet}
    </EmptyState>
  {:else}
    <!-- Keep the previous table visible (dimmed) during refetches so the
           page doesn't jump and pagination stays in place. -->
    <DimWrapper active={res.loading} class="flex flex-col gap-4">
      <TableShell>
        <Table.Header>
          <Table.Row class="bg-muted/75">
            <Table.Head class="text-muted-foreground">Date</Table.Head>
            <Table.Head class="text-muted-foreground">Change</Table.Head>
            <Table.Head class="text-muted-foreground">Source</Table.Head>
            <Table.Head class="w-24 text-right text-muted-foreground">Action</Table.Head>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {#each res.data.entries as entry (entry.id)}
            {@const revertible = entry.id === res.data.revertibleId}
            {@const impact = impactLabel(entry)}
            <Table.Row>
              <Table.Cell class="py-2.5 whitespace-nowrap">
                <span class="tabular-nums">
                  {formatDate(entry.occurredAt, {
                    month: 'short',
                    day: 'numeric',
                    year: 'numeric',
                    hour: 'numeric',
                    minute: '2-digit'
                  })}
                </span>
              </Table.Cell>
              <Table.Cell class="py-2.5">
                <span class={entry.revertedBy ? 'text-muted-foreground line-through' : ''}>
                  {entry.label}
                </span>
                {#if entry.revertedBy}
                  <span class="ml-1.5 text-xs text-muted-foreground">(reverted)</span>
                {/if}
                {#if impact}
                  <span class="block text-xs text-muted-foreground/70 tabular-nums">
                    {impact}
                  </span>
                {/if}
              </Table.Cell>
              <Table.Cell class="py-2.5 whitespace-nowrap">
                <Badge variant={entry.kind === 'import' ? 'secondary' : 'outline'}>
                  {sourceLabel(entry)}
                </Badge>
              </Table.Cell>
              <Table.Cell class="py-2.5 text-right">
                {#if revertible}
                  <Button
                    variant="ghost"
                    size="sm"
                    onclick={() => openConfirm(entry)}
                    disabled={reverting}
                    title="Revert this change"
                  >
                    <Undo2 aria-hidden="true" class="size-3.5" />
                    Revert
                  </Button>
                {:else}
                  <span class="text-xs text-muted-foreground">{EMPTY_DASH}</span>
                {/if}
              </Table.Cell>
            </Table.Row>
          {/each}
        </Table.Body>
      </TableShell>
      <PaginationNav
        bind:page
        totalPages={res.data.totalPages}
        totalItems={res.data.total}
        pageSize={appSettings.value.pageSize}
        onPageSizeChange={(size) => applyPageSize(size, (v) => (page = v))}
      />
    </DimWrapper>
  {/if}
{:else if res.error}
  <ErrorCard message={res.error} onRetry={() => res.reload()} />
{/if}

<Dialog.Root bind:open={confirmOpen}>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>Revert this change?</Dialog.Title>
      <Dialog.Description>
        {#if pending}
          "{pending.label}" will be undone. This restores the data to how it was before this change.
          Only the latest change can be reverted.
        {/if}
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button variant="outline" onclick={closeConfirm} disabled={reverting}>Cancel</Button>
      <Button variant="destructive" onclick={handleRevert} disabled={reverting || !pending}>
        {#if reverting}
          <LoaderCircle aria-hidden="true" class="size-3 animate-spin" />
          Reverting…
        {:else}
          <Undo2 aria-hidden="true" class="size-3" />
          Revert change
        {/if}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
