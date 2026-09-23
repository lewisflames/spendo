<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';

  /**
   * Standard create/edit footer for dialogs: two-step Delete (edit mode only),
   * Cancel and Save. Delete confirmation text lives in the body. Optional
   * `onSaveAndNew` adds a batch-entry button (create mode) and `onDuplicate`
   * adds an edit-mode shortcut to the new-transaction template. Page forms use
   * header actions instead — no sticky bar.
   */
  let {
    mode = 'create',
    saving,
    deleting,
    confirmDelete,
    saveLabel,
    saveDisabled,
    savingAndNew = false,
    saveAndNewDisabled = false,
    onDelete,
    onSave,
    onSaveAndNew,
    onDuplicate,
    onCancel
  }: {
    mode?: 'create' | 'edit';
    saving: boolean;
    deleting: boolean;
    confirmDelete: boolean;
    saveLabel: string;
    saveDisabled: boolean;
    savingAndNew?: boolean;
    saveAndNewDisabled?: boolean;
    onDelete?: () => void;
    onSave: () => void;
    onSaveAndNew?: () => void;
    onDuplicate?: () => void;
    onCancel: () => void;
  } = $props();
</script>

{#snippet buttons()}
  {#if mode === 'edit' && onDelete}
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
  {#if mode === 'edit' && onDuplicate}
    <Button type="button" variant="outline" onclick={onDuplicate} disabled={saving || deleting}>
      Duplicate
    </Button>
  {/if}
  <Button type="button" variant="outline" onclick={onCancel}>Cancel</Button>
  {#if onSaveAndNew}
    <Button
      type="button"
      variant="outline"
      onclick={onSaveAndNew}
      disabled={saveAndNewDisabled || saving || savingAndNew || deleting}
    >
      {savingAndNew ? 'Saving…' : 'Save & add another'}
    </Button>
  {/if}
  <Button type="button" onclick={onSave} disabled={saveDisabled}>
    {saving ? 'Saving…' : saveLabel}
  </Button>
{/snippet}

<Dialog.Footer class="flex-col gap-2 sm:flex-row">
  {@render buttons()}
</Dialog.Footer>
