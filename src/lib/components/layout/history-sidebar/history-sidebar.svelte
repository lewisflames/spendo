<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { History, Redo2, Undo2 } from '@lucide/svelte';
  import ChangeHistory from '$lib/features/history/change-history.svelte';
  import {
    changeHistory,
    redoLatestChange,
    undoLatestChange
  } from '$lib/features/history/history.svelte';
</script>

<aside
  id="changes-history-panel"
  aria-label="Changes history"
  class="flex w-80 shrink-0 flex-col overflow-hidden bg-sidebar"
>
  <header class="flex h-10 shrink-0 items-center justify-between gap-2 px-3">
    <div class="flex min-w-0 items-center gap-2">
      <History aria-hidden="true" class="size-4 shrink-0 text-muted-foreground" />
      <h2 class="truncate text-sm font-semibold">History</h2>
    </div>
    <div class="flex items-center gap-0.5">
      <Button
        variant="ghost"
        size="icon-sm"
        class="text-muted-foreground"
        aria-label="Undo last change"
        title="Undo last change (Ctrl+Z)"
        disabled={changeHistory.value.undoableId === null || changeHistory.value.busy}
        onclick={() => void undoLatestChange()}
      >
        <Undo2 class="size-4" />
      </Button>
      <Button
        variant="ghost"
        size="icon-sm"
        class="text-muted-foreground"
        aria-label="Redo last change"
        title="Redo last change (Ctrl+Shift+Z)"
        disabled={changeHistory.value.redoableId === null || changeHistory.value.busy}
        onclick={() => void redoLatestChange()}
      >
        <Redo2 class="size-4" />
      </Button>
    </div>
  </header>
  <div class="flex min-h-0 flex-1 flex-col">
    <ChangeHistory />
  </div>
</aside>
