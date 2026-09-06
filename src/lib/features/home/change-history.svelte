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
  import ImportButton from '$lib/components/shared/import-button.svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { appSettings, saveSettings } from '$lib/stores/settings.svelte';
  import { fetchChangeHistory, revertHistoryEntry } from '$lib/tauri';
  import type { ChangeEntry } from '$lib/tauri';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { formatDate, plural } from '$lib/utils/format.svelte';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';

  let page = $state(1);
  let pageSize = $derived(appSettings.value.pageSize);

  const res = useAsyncData({
    enabled: () => !!dbSession.value,
    fetch: async () => {
      // Subscribe to summary changes so header imports/reimports refresh
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

  let loading = $derived(res.loading);
  let data = $derived(res.data);
  let error = $derived(res.error);

  let confirmOpen = $state(false);
  let pending = $state<ChangeEntry | null>(null);
  let reverting = $state(false);

  function handlePageSizeChange(size: number) {
    saveSettings({ pageSize: size });
    page = 1;
  }

  function sourceLabel(entry: ChangeEntry): string {
    if (entry.kind === 'revert') return 'Undo';
    if (entry.kind === 'import') return 'Import';
    return 'Manual';
  }

  function impactLabel(entry: ChangeEntry): string | null {
    if (entry.kind !== 'import') return null;
    if (!entry.transactionsAffected && !entry.accountsAffected) return null;
    const parts: string[] = [];
    if (entry.transactionsAffected) {
      parts.push(plural(entry.transactionsAffected, 'transaction'));
    }
    if (entry.accountsAffected) {
      parts.push(plural(entry.accountsAffected, 'account'));
    }
    return parts.join(' · ');
  }

  function openConfirm(entry: ChangeEntry) {
    pending = entry;
    confirmOpen = true;
  }

  async function handleRevert() {
    if (!pending || reverting) return;
    reverting = true;
    try {
      const result = await revertHistoryEntry(pending.id);
      dbSession.value = { summary: result.summary };
      toast.success('Change reverted');
      confirmOpen = false;
      pending = null;
      res.reload();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Revert failed'));
    } finally {
      reverting = false;
    }
  }
</script>

{#if loading && !data}
  <TableSkeleton cols={4} />
{:else if error}
  <ErrorCard message={error} onRetry={() => res.reload()} />
{:else if data}
  {#if data.entries.length === 0}
    <EmptyState
      card
      title="No changes yet"
      description="Import from GnuCash to start building history."
    >
      {#snippet icon()}
        <History aria-hidden="true" class="text-muted-foreground" />
      {/snippet}
      {#snippet action()}
        <ImportButton />
      {/snippet}
    </EmptyState>
  {:else}
    <!-- Keep the previous table visible (dimmed) during refetches so the
           page doesn't jump and pagination stays in place. -->
    <DimWrapper active={loading} class="flex flex-col gap-4">
      <TableShell>
        <Table.Header>
          <Table.Row class="bg-muted/75">
            <Table.Head>Date</Table.Head>
            <Table.Head>Change</Table.Head>
            <Table.Head>Source</Table.Head>
            <Table.Head class="w-24 text-right">Action</Table.Head>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {#each data.entries as entry (entry.id)}
            {@const revertible = entry.id === data.revertibleId}
            {@const impact = impactLabel(entry)}
            <Table.Row class="hover:bg-muted/50">
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
                  <span class="text-xs text-muted-foreground">—</span>
                {/if}
              </Table.Cell>
            </Table.Row>
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
      <Button variant="outline" onclick={() => (confirmOpen = false)} disabled={reverting}>
        Cancel
      </Button>
      <Button onclick={handleRevert} disabled={reverting || !pending}>
        {#if reverting}
          <LoaderCircle aria-hidden="true" class="size-3 animate-spin" />
          Reverting...
        {:else}
          <Undo2 aria-hidden="true" class="size-3" />
          Revert change
        {/if}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
