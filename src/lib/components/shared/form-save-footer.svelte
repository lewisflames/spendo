<script lang="ts">
  import { Button } from '$lib/components/ui/button';

  let {
    mode = 'create',
    saving,
    deleting,
    confirmDelete,
    saveLabel,
    savingLabel = 'Saving…',
    saveDisabled,
    onDelete,
    onSave,
    onCancel
  }: {
    mode?: 'create' | 'edit';
    saving: boolean;
    deleting: boolean;
    confirmDelete: boolean;
    saveLabel: string;
    savingLabel?: string;
    saveDisabled: boolean;
    onDelete: () => void;
    onSave: () => void;
    onCancel: () => void;
  } = $props();
</script>

<div
  class="sticky bottom-0 z-10 flex flex-col gap-2 border-t bg-background/95 py-4 backdrop-blur sm:flex-row sm:justify-end"
>
  {#if mode === 'edit'}
    <Button
      type="button"
      variant={confirmDelete ? 'destructive' : 'outline'}
      onclick={onDelete}
      disabled={saving || deleting}
      class="sm:mr-auto"
    >
      {deleting ? 'Deleting…' : confirmDelete ? 'Confirm delete' : 'Delete'}
    </Button>
  {/if}
  <Button type="button" variant="outline" onclick={onCancel}>Cancel</Button>
  <Button type="button" onclick={onSave} disabled={saveDisabled}>
    {saving ? savingLabel : saveLabel}
  </Button>
</div>
