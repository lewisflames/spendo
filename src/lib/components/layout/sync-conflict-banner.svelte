<script lang="ts">
  import { FileWarning, X } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { tooltip } from '$lib/attachments/tooltip';
  import { openBookAt } from '$lib/features/book/book-actions';
  import { conflictReviewHref } from '$lib/features/sync-conflicts/sync-conflict-utils';
  import { dismissSyncConflict } from '$lib/stores/sync-conflicts.svelte';
  import { formatDate } from '$lib/utils/format.svelte';
  import type { SyncConflictCopy } from '$lib/tauri';

  // A sync tool (Syncthing) kept another device's version of this book as a
  // conflict copy next to it. Nothing is merged or deleted: the user reviews,
  // opens it, or dismisses the hint for that file.
  let { copies }: { copies: SyncConflictCopy[] } = $props();

  let copy = $derived(copies[0]);
  let more = $derived(copies.length - 1);
</script>

{#if copy}
  <div
    role="status"
    class="mb-(--page-gap) flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border border-warning/40 bg-warning/10 px-3.5 py-2.5 text-sm"
  >
    <FileWarning aria-hidden="true" class="size-4 shrink-0 text-warning" />
    <p class="min-w-0 flex-1">
      <span class="font-medium" {@attach tooltip(copy.fileName)}>
        {#if copy.savedAt}
          A conflicting copy from {formatDate(copy.savedAt, {
            dateStyle: 'medium',
            timeStyle: 'short'
          })} exists.
        {:else}
          A conflicting copy of this book exists.
        {/if}
      </span>
      Another device’s changes may be missing here{#if more > 0}
        ({more === 1 ? 'one more copy' : `${more} more copies`} too){/if}
    </p>
    <div class="flex items-center gap-1.5">
      <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
      <Button variant="outline" size="sm" href={conflictReviewHref(copy.fileName)}>Review</Button>
      <Button variant="ghost" size="sm" onclick={() => void openBookAt(copy.path)}>
        Open that copy
      </Button>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label="Dismiss"
        onclick={() => dismissSyncConflict(copy.path)}
      >
        <X aria-hidden="true" class="size-4" />
      </Button>
    </div>
  </div>
{/if}
