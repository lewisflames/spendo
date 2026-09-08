<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';

  /**
   * Standard create/edit dialog footer: two-step Delete (edit mode only),
   * Cancel, and Save. Delete confirmation text lives in the dialog body;
   * this only owns the buttons.
   */
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

<Dialog.Footer class="flex-col gap-2 sm:flex-row">
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
</Dialog.Footer>
