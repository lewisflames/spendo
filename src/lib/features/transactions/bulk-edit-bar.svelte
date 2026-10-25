<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { ArrowRightLeft, PencilLine, Store, Tag, Trash2, X } from '@lucide/svelte';
  import { plural } from '$lib/utils/format.svelte';
  import type { BulkAction } from '$lib/tauri';

  let {
    count,
    total,
    pageFullySelected,
    allMatching,
    onSelectAllMatching,
    onClear,
    onAction
  }: {
    count: number;
    /** Every transaction matching the current filters. */
    total: number;
    pageFullySelected: boolean;
    allMatching: boolean;
    onSelectAllMatching: () => void;
    onClear: () => void;
    onAction: (kind: BulkAction['kind']) => void;
  } = $props();
</script>

<div
  role="toolbar"
  aria-label="Bulk actions"
  class="sticky bottom-4 z-10 flex flex-wrap items-center gap-2 rounded-lg border bg-background/95 px-3 py-2 shadow-lg backdrop-blur"
>
  <p class="text-sm font-medium tabular-nums" aria-live="polite">
    {allMatching ? `All ${plural(total, 'transaction')} selected` : `${count} selected`}
  </p>
  {#if pageFullySelected && !allMatching && total > count}
    <Button variant="link" size="sm" class="h-auto px-0" onclick={onSelectAllMatching}>
      Select all {plural(total, 'matching transaction')}
    </Button>
  {/if}
  <div class="ml-auto flex flex-wrap items-center gap-2">
    <Button variant="outline" size="sm" onclick={() => onAction('moveSplits')}>
      <ArrowRightLeft aria-hidden="true" />
      Move
    </Button>
    <Button variant="outline" size="sm" onclick={() => onAction('setDescription')}>
      <PencilLine aria-hidden="true" />
      Rename
    </Button>
    <Button variant="outline" size="sm" onclick={() => onAction('setPayee')}>
      <Store aria-hidden="true" />
      Payee
    </Button>
    <Button variant="outline" size="sm" onclick={() => onAction('addTag')}>
      <Tag aria-hidden="true" />
      Tag
    </Button>
    <Button variant="destructive" size="sm" onclick={() => onAction('delete')}>
      <Trash2 aria-hidden="true" />
      Delete
    </Button>
    <Button variant="ghost" size="icon-sm" aria-label="Clear selection" onclick={onClear}>
      <X aria-hidden="true" />
    </Button>
  </div>
</div>
