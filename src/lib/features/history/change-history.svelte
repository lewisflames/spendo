<script lang="ts">
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Skeleton } from '$lib/components/ui/skeleton';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import DimWrapper from '$lib/components/shared/dim-wrapper.svelte';
  import { History, RefreshCw } from '@lucide/svelte';
  import type { ChangeEntry } from '$lib/tauri';
  import { reimportGnucashPath } from '$lib/features/book/import-export-actions';
  import { formatDate, formatRelativeTime, parseLocalDate } from '$lib/utils/format.svelte';
  import { changeHistory, refreshChangeHistory } from './history.svelte';
  import { groupHistoryEntries } from './date-groups';
  import { sourceLabel } from './history-labels';

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

  const groups = $derived(groupHistoryEntries(changeHistory.value.entries));
</script>

{#if changeHistory.value.loading && changeHistory.value.entries.length === 0}
  <div class="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto p-3">
    {#each Array(5) as _, i (i)}
      <Skeleton class="h-16 w-full rounded-lg" />
    {/each}
  </div>
{:else}
  <div class="min-h-0 flex-1 overflow-y-auto p-3">
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
            {#each group.entries as entry (entry.id)}
              <div class="rounded-lg border bg-card p-2.5">
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
                  {#if entry.revertedBy}
                    <span class="text-xs text-muted-foreground">undone</span>
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
                      Re-run import
                    </Button>
                  {/if}
                </div>
              </div>
            {/each}
          </section>
        {/each}
      </DimWrapper>
    {/if}
  </div>
{/if}
