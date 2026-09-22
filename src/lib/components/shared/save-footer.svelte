<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';

  /**
   * Standard create/edit footer: two-step Delete (edit mode only), Cancel and
   * Save. `layout="dialog"` renders inside `Dialog.Footer`; `layout="form"`
   * renders the sticky page-form bar. Delete confirmation text lives in the
   * body.
   */
  let {
    layout = 'dialog',
    mode = 'create',
    saving,
    deleting,
    confirmDelete,
    saveLabel,
    saveDisabled,
    onDelete,
    onSave,
    onCancel
  }: {
    layout?: 'dialog' | 'form';
    mode?: 'create' | 'edit';
    saving: boolean;
    deleting: boolean;
    confirmDelete: boolean;
    saveLabel: string;
    saveDisabled: boolean;
    onDelete?: () => void;
    onSave: () => void;
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
  <Button type="button" variant="outline" onclick={onCancel}>Cancel</Button>
  <Button type="button" onclick={onSave} disabled={saveDisabled}>
    {saving ? 'Saving…' : saveLabel}
  </Button>
{/snippet}

{#if layout === 'dialog'}
  <Dialog.Footer class="flex-col gap-2 sm:flex-row">
    {@render buttons()}
  </Dialog.Footer>
{:else}
  <div
    class="sticky bottom-0 z-10 flex flex-col gap-2 border-t bg-background/95 py-4 backdrop-blur sm:flex-row sm:justify-end"
  >
    {@render buttons()}
  </div>
{/if}
