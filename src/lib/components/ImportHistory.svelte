<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import * as Item from '$lib/components/ui/item';
  import { Badge } from '$lib/components/ui/badge';
  import { Upload, Trash2 } from '@lucide/svelte';
  import { dbSession } from '$lib/stores/db.svelte';
  import { fetchImportHistory, deleteImportLog } from '$lib/db.remote';
  import { formatRelativeDate, formatDateGroup } from '$lib/format.svelte';
  import type { ImportLogDetails } from '$lib/server/import-gnucash';

  let importHistory = $state<ImportLogDetails[] | null>(null);
  let loading = $state(true);

  $effect(() => {
    if (!dbSession.value) {
      importHistory = null;
      loading = false;
      return;
    }
    loading = true;
    fetchImportHistory({}).then((h) => {
      importHistory = h;
      loading = false;
    });
  });

  let groups = $derived(groupByDate(importHistory ?? []));

  function groupByDate(
    entries: ImportLogDetails[]
  ): { label: string; entries: ImportLogDetails[] }[] {
    const groups: Record<string, ImportLogDetails[]> = {};
    const order = ['Today', 'Yesterday', 'This week', 'Older'];
    for (const e of entries) {
      const label = formatDateGroup(e.importedAt);
      (groups[label] ??= []).push(e);
    }
    return order.filter((l) => groups[l]).map((label) => ({ label, entries: groups[label] }));
  }

  function buildSentence(e: ImportLogDetails): string {
    const hasAnyChange =
      e.accountsAdded > 0 ||
      e.transactionsAdded > 0 ||
      e.splitsAdded > 0 ||
      e.accountsRemoved > 0 ||
      e.transactionsRemoved > 0 ||
      e.splitsRemoved > 0;
    if (!hasAnyChange) {
      return 'No changes, all GUIDs matched.';
    }

    const added: string[] = [];
    if (e.accountsAdded > 0)
      added.push(`${e.accountsAdded} account${e.accountsAdded > 1 ? 's' : ''}`);
    if (e.transactionsAdded > 0)
      added.push(`${e.transactionsAdded} transaction${e.transactionsAdded > 1 ? 's' : ''}`);
    if (e.splitsAdded > 0) added.push(`${e.splitsAdded} split${e.splitsAdded > 1 ? 's' : ''}`);

    const removed: string[] = [];
    if (e.accountsRemoved > 0)
      removed.push(`${e.accountsRemoved} account${e.accountsRemoved > 1 ? 's' : ''}`);
    if (e.transactionsRemoved > 0)
      removed.push(`${e.transactionsRemoved} transaction${e.transactionsRemoved > 1 ? 's' : ''}`);
    if (e.splitsRemoved > 0)
      removed.push(`${e.splitsRemoved} split${e.splitsRemoved > 1 ? 's' : ''}`);

    const parts: string[] = [];
    if (added.length > 0) parts.push(`Added ${formatList(added)}.`);
    if (removed.length > 0) parts.push(`Removed ${formatList(removed)}.`);
    return parts.join(' ');
  }

  function formatList(items: string[]): string {
    if (items.length === 1) return items[0];
    if (items.length === 2) return `${items[0]} and ${items[1]}`;
    return `${items.slice(0, -1).join(', ')}, and ${items[items.length - 1]}`;
  }

  function handleRemove(id: number) {
    deleteImportLog({ id }).then(() => {
      importHistory = (importHistory ?? []).filter((e) => e.id !== id);
    });
  }
</script>

{#if loading}
  <Item.Root variant="outline">
    <Item.Content class="py-6 text-center text-sm text-muted-foreground">Loading…</Item.Content>
  </Item.Root>
{:else if importHistory === null || importHistory.length === 0}
  <Item.Root variant="outline">
    <Item.Content class="flex flex-col items-center gap-3 py-10 text-center">
      <div class="flex size-12 items-center justify-center rounded-full bg-muted">
        <Upload aria-hidden="true" class="size-6 text-muted-foreground" />
      </div>
      <div class="flex flex-col gap-1">
        <p class="font-medium text-foreground">No imports yet</p>
        <p class="text-sm text-muted-foreground">
          Import a .gnucash file to start analyzing your finances.
        </p>
      </div>
    </Item.Content>
  </Item.Root>
{:else}
  <div class="flex flex-col gap-4">
    {#each groups as { label, entries } (label)}
      <section class="flex flex-col gap-2">
        <h4 class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
          {label}
        </h4>
        {#each entries as entry, i (entry.importedAt)}
          {@const isLatest = i === 0 && label === groups[0].label}
          <Card.Root size="sm" class="relative {isLatest ? 'border-primary/25' : ''}">
            <Card.Content class="flex flex-col gap-1.5">
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-2">
                  {#if isLatest}
                    <Badge variant="default" class="text-[10px] leading-none h-4 px-1.5"
                      >Latest</Badge
                    >
                  {/if}
                  <span class="text-xs text-muted-foreground"
                    >{formatRelativeDate(entry.importedAt)}</span
                  >
                </div>
                <button
                  onclick={() => handleRemove(entry.id)}
                  class="text-muted-foreground hover:text-destructive transition-colors"
                  aria-label="Remove import entry"
                >
                  <Trash2 class="size-3.5" />
                </button>
              </div>
              <p class="text-sm text-foreground/90 pl-0.5">{buildSentence(entry)}</p>
            </Card.Content>
          </Card.Root>
        {/each}
      </section>
    {/each}
  </div>
{/if}
