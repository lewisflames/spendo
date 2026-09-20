<script lang="ts">
  import { onMount } from 'svelte';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import * as Dialog from '$lib/components/ui/dialog';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import { History, Info, Loader2, Redo2, RefreshCw, Undo2 } from '@lucide/svelte';
  import type { ChangeEntry, ChangeEntryDetail } from '$lib/tauri';
  import { fetchChangeEntry } from '$lib/tauri';
  import { reimportGnucashPath } from '$lib/features/book/import-export-actions';
  import { formatDate, formatRelativeTime, parseLocalDate } from '$lib/utils/format.svelte';
  import { errMsg } from '$lib/utils/error';
  import {
    changeHistory,
    loadOlderHistory,
    refreshChangeHistory,
    redoLatestChange,
    undoLatestChange
  } from './history.svelte';
  import { groupHistoryEntries } from './date-groups';
  import { impactLabel, sourceLabel } from './history-labels';

  /** Absolute timestamp for the relative-time tooltip, locale-formatted. */
  function exactTime(entry: ChangeEntry): string {
    return formatDate(entry.occurredAt, {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
      hour: 'numeric',
      minute: '2-digit'
    });
  }

  function relativeTime(entry: ChangeEntry): string {
    return formatRelativeTime(parseLocalDate(entry.occurredAt).getTime() / 1000);
  }

  /** GnuCash imports can be re-run from their recorded source file, unless undone. */
  function canReimport(entry: ChangeEntry): boolean {
    return (
      entry.kind === 'import' &&
      entry.source === 'import' &&
      entry.sourcePath !== null &&
      entry.revertedBy === null
    );
  }

  // Re-render the date buckets while the panel stays open, so "Today" rolls
  // over at midnight instead of waiting for the next entry.
  let now = $state(new Date());
  onMount(() => {
    const timer = setInterval(() => (now = new Date()), 60_000);
    return () => clearInterval(timer);
  });

  const groups = $derived(groupHistoryEntries(changeHistory.value.entries, now));

  // ── Detail dialog ──────────────────────────────────────────────────────────
  let detailOpen = $state(false);
  let detailEntry = $state<ChangeEntry | null>(null);
  let detail = $state<ChangeEntryDetail | null>(null);
  let detailLoading = $state(false);
  let detailError = $state<string | null>(null);

  async function openDetail(entry: ChangeEntry) {
    detailEntry = entry;
    detail = null;
    detailError = null;
    detailLoading = true;
    detailOpen = true;
    try {
      const data = await fetchChangeEntry(entry.id);
      if (detailEntry?.id === entry.id) detail = data;
    } catch (e: unknown) {
      if (detailEntry?.id === entry.id) detailError = errMsg(e, 'Failed to load change details');
    } finally {
      if (detailEntry?.id === entry.id) detailLoading = false;
    }
  }

  /** Human labels for the known `detail_json` keys written by the backend. */
  const DETAIL_LABELS: Record<string, string> = {
    filePath: 'File',
    filename: 'Filename',
    created: 'Rows imported',
    rulesApplied: 'Rules applied',
    accountGuid: 'Account',
    statementDate: 'Statement date',
    endingBalance: 'Ending balance',
    startingBalance: 'Starting balance',
    clearedCount: 'Cleared splits',
    markedCleared: 'Marked cleared',
    markedNew: 'Marked new',
    scheduleGuid: 'Schedule',
    scheduleName: 'Schedule name',
    postDate: 'Post date',
    targetAccountGuid: 'Target account',
    offsetAccountGuid: 'Offset account',
    sha256: 'File hash'
  };

  function detailRows(detail: Record<string, unknown>): [string, string][] {
    return Object.entries(detail).map(([key, value]) => [
      DETAIL_LABELS[key] ?? key,
      typeof value === 'string' || typeof value === 'number' ? String(value) : JSON.stringify(value)
    ]);
  }

  const TABLE_LABELS: Record<string, string> = {
    commodities: 'commodities',
    accounts: 'accounts',
    transactions: 'transactions',
    splits: 'splits',
    recurring_schedules: 'schedules'
  };

  function tableImpact(row: ChangeEntryDetail['tables'][number]): string {
    const parts: string[] = [];
    if (row.added) parts.push(`${row.added} added`);
    if (row.changed) parts.push(`${row.changed} changed`);
    if (row.removed) parts.push(`${row.removed} removed`);
    return parts.join(' · ');
  }
</script>

{#if changeHistory.value.loading && changeHistory.value.entries.length === 0}
  <div class="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto py-2 pr-2 pl-0">
    {#each Array(5) as _, i (i)}
      <Skeleton class="h-16 w-full rounded-lg" />
    {/each}
  </div>
{:else}
  <div class="min-h-0 flex-1 overflow-y-auto py-2 pr-2 pl-0">
    {#if changeHistory.value.error}
      <ErrorCard message={changeHistory.value.error} onRetry={() => void refreshChangeHistory()} />
    {/if}

    {#if changeHistory.value.entries.length === 0}
      {#if !changeHistory.value.error}
        <EmptyState title="No changes yet" description="Imports, edits and undos will appear here.">
          {#snippet icon()}
            <History aria-hidden="true" class="text-muted-foreground" />
          {/snippet}
        </EmptyState>
      {/if}
    {:else}
      <!-- Keep the previous list visible (dimmed) during refetches so the
           panel doesn't jump. -->
      <DimWrapper active={changeHistory.value.loading} class="flex flex-col gap-4">
        {#each groups as group (group.key)}
          <section class="flex flex-col gap-2">
            <h3 class="px-1 text-xs font-medium text-muted-foreground">{group.label}</h3>
            <ul class="flex list-none flex-col gap-2">
              {#each group.entries as entry (entry.id)}
                {@const impact = impactLabel(entry)}
                {@const undoable = changeHistory.value.undoableId === entry.id}
                {@const redoable = changeHistory.value.redoableId === entry.id}
                <li class="rounded-lg border bg-card p-2.5">
                  <p
                    class="min-w-0 text-sm leading-snug {entry.revertedBy
                      ? 'text-muted-foreground line-through'
                      : ''}"
                  >
                    {entry.label}
                  </p>
                  <div class="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1">
                    <time
                      class="text-xs text-muted-foreground tabular-nums"
                      datetime={entry.occurredAt}
                      title={exactTime(entry)}
                    >
                      {relativeTime(entry)}
                    </time>
                    <Badge variant={entry.kind === 'import' ? 'secondary' : 'outline'}>
                      {sourceLabel(entry)}
                    </Badge>
                    {#if impact}
                      <span class="text-xs text-muted-foreground">{impact}</span>
                    {/if}
                    {#if entry.revertedBy}
                      <span class="text-xs text-muted-foreground">undone</span>
                    {/if}
                    <div class="ms-auto flex items-center gap-0.5">
                      {#if undoable}
                        <Button
                          variant="ghost"
                          size="sm"
                          class="h-6 gap-1 px-1.5 text-xs text-muted-foreground"
                          title="Undo this change (Ctrl+Z)"
                          disabled={changeHistory.value.busy}
                          onclick={() => void undoLatestChange()}
                        >
                          <Undo2 aria-hidden="true" class="size-3" />
                          Undo
                        </Button>
                      {:else if redoable}
                        <Button
                          variant="ghost"
                          size="sm"
                          class="h-6 gap-1 px-1.5 text-xs text-muted-foreground"
                          title="Redo this change (Ctrl+Shift+Z)"
                          disabled={changeHistory.value.busy}
                          onclick={() => void redoLatestChange()}
                        >
                          <Redo2 aria-hidden="true" class="size-3" />
                          Redo
                        </Button>
                      {/if}
                      {#if canReimport(entry)}
                        <Button
                          variant="ghost"
                          size="sm"
                          class="h-6 gap-1 px-1.5 text-xs text-muted-foreground"
                          title="Re-run this import"
                          onclick={() =>
                            void reimportGnucashPath(
                              entry.sourcePath!,
                              entry.filename ?? 'book.gnucash'
                            )}
                        >
                          <RefreshCw aria-hidden="true" class="size-3" />
                          Re-run
                        </Button>
                      {/if}
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        class="size-6 text-muted-foreground"
                        aria-label="Change details"
                        title="Details"
                        onclick={() => void openDetail(entry)}
                      >
                        <Info aria-hidden="true" class="size-3" />
                      </Button>
                    </div>
                  </div>
                </li>
              {/each}
            </ul>
          </section>
        {/each}

        {#if changeHistory.value.hasMore}
          <Button
            variant="outline"
            size="sm"
            class="w-full"
            disabled={changeHistory.value.loadingMore}
            onclick={() => void loadOlderHistory()}
          >
            {#if changeHistory.value.loadingMore}
              <Loader2 aria-hidden="true" class="size-3 animate-spin" />
              Loading…
            {:else}
              Load older changes
            {/if}
          </Button>
        {/if}
      </DimWrapper>
    {/if}
  </div>
{/if}

<Dialog.Root bind:open={detailOpen}>
  <Dialog.Content class="sm:max-w-md">
    <Dialog.Header>
      <Dialog.Title class="truncate">{detailEntry?.label ?? 'Change details'}</Dialog.Title>
      <Dialog.Description>
        {#if detailEntry}
          {exactTime(detailEntry)} · {sourceLabel(detailEntry)}
        {/if}
      </Dialog.Description>
    </Dialog.Header>
    {#if detailLoading}
      <p class="py-2 text-sm text-muted-foreground">Loading details…</p>
    {:else if detailError}
      <ErrorCard message={detailError} />
    {:else if detail}
      <div class="flex flex-col gap-4 py-2">
        {#if detail.detail && Object.keys(detail.detail).length > 0}
          <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-sm">
            {#each detailRows(detail.detail) as [label, value] (label)}
              <dt class="text-muted-foreground">{label}</dt>
              <dd class="min-w-0 break-words">{value}</dd>
            {/each}
          </dl>
        {:else}
          <p class="text-sm text-muted-foreground">No extra details recorded for this change.</p>
        {/if}
        {#if detail.tables.length > 0}
          <div class="flex flex-col gap-1">
            <h4 class="text-xs font-medium text-muted-foreground">Rows affected</h4>
            {#each detail.tables as table (table.tableName)}
              <div class="flex items-center justify-between gap-2 text-sm">
                <span>{TABLE_LABELS[table.tableName] ?? table.tableName}</span>
                <span class="text-muted-foreground">{tableImpact(table)}</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
