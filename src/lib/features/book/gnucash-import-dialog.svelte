<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import { LoaderCircle } from '@lucide/svelte';
  import {
    closeGnucashImportDialog,
    gnucashImportDialog
  } from './gnucash-import-dialog-state.svelte';
  import { confirmGnucashImport } from './import-export-actions';
  import { plural } from '$lib/utils/format.svelte';

  let deleteOrphans = $state(true);

  useDialogReset(
    () => gnucashImportDialog.value.open,
    () => {
      deleteOrphans = true;
    }
  );

  const preview = $derived(gnucashImportDialog.value.preview);
  const newCount = $derived(preview ? preview.accountsAdded + preview.transactionsAdded : 0);
  const updatedCount = $derived(
    preview ? preview.accountsUpdated + preview.transactionsUpdated : 0
  );
  const removedCount = $derived(
    preview ? preview.accountsOrphaned + preview.transactionsOrphaned : 0
  );
  const hasRemovals = $derived(removedCount > 0);
</script>

{#if preview}
  <Dialog.Root bind:open={gnucashImportDialog.value.open}>
    <Dialog.Content
      class="max-h-[90vh] overflow-y-auto sm:max-w-lg"
      escapeKeydownBehavior={gnucashImportDialog.value.importing ? 'ignore' : 'close'}
      interactOutsideBehavior={gnucashImportDialog.value.importing ? 'ignore' : 'close'}
      showCloseButton={!gnucashImportDialog.value.importing}
    >
      <Dialog.Header>
        <Dialog.Title>Import {preview.filename}?</Dialog.Title>
        <Dialog.Description>We’ll fold this file into the book you’ve got open.</Dialog.Description>
      </Dialog.Header>

      <div class="grid grid-cols-3 gap-3 rounded-md border p-3">
        <div>
          <p class="font-medium">{newCount} new</p>
          <p class="text-xs text-muted-foreground">accounts &amp; transactions</p>
        </div>
        <div>
          <p class="font-medium">{updatedCount} updated</p>
          <p class="text-xs text-muted-foreground">accounts &amp; transactions</p>
        </div>
        <div>
          <p class="font-medium">{removedCount} removed</p>
          <p class="text-xs text-muted-foreground">accounts &amp; transactions</p>
        </div>
      </div>

      {#if preview.schedulesIgnored > 0}
        <p class="text-sm text-muted-foreground">
          {plural(preview.schedulesIgnored, 'scheduled transaction')} won’t be imported. Recreate the
          ones you need under Scheduled.
        </p>
      {/if}

      {#if hasRemovals}
        <div class="rounded-md border border-destructive/30 bg-destructive/5 p-3 text-sm">
          <p>Some of what’s in your book isn’t in this file.</p>
          <p class="mt-2 text-muted-foreground">
            {preview.accountsOrphaned} accounts · {preview.transactionsOrphaned} transactions would be
            removed.
          </p>
          {#if preview.orphanAccountNames.length > 0}
            <p class="mt-2 text-muted-foreground">
              Accounts not in the source file: {preview.orphanAccountNames.join(', ')}
            </p>
          {/if}
          <div class="mt-3 flex items-start gap-2">
            <Checkbox id="gnucash-delete-orphans" bind:checked={deleteOrphans} />
            <label for="gnucash-delete-orphans" class="font-medium">
              Also remove what’s missing from the file
            </label>
          </div>
        </div>
      {/if}

      <Dialog.Footer>
        <Button
          variant="outline"
          onclick={closeGnucashImportDialog}
          disabled={gnucashImportDialog.value.importing}
        >
          Cancel
        </Button>
        <Button
          variant={hasRemovals && deleteOrphans ? 'destructive' : 'default'}
          disabled={gnucashImportDialog.value.importing}
          onclick={() => void confirmGnucashImport(hasRemovals ? deleteOrphans : true)}
        >
          {#if gnucashImportDialog.value.importing}
            <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
            Importing…
          {:else}
            Import
          {/if}
        </Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>
{/if}
