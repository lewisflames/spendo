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

  let deleteOrphans = $state(true);

  useDialogReset(
    () => gnucashImportDialog.open,
    () => {
      deleteOrphans = true;
    }
  );

  const preview = $derived(gnucashImportDialog.preview);
  const newCount = $derived(preview ? preview.accountsAdded + preview.transactionsAdded : 0);
  const updatedCount = $derived(
    preview ? preview.accountsUpdated + preview.transactionsUpdated : 0
  );
  const removedCount = $derived(
    preview ? preview.accountsOrphaned + preview.transactionsOrphaned : 0
  );
  const hasRemovals = $derived(removedCount > 0);
  const scheduleCount = $derived(preview ? preview.schedulesAdded + preview.schedulesUpdated : 0);
  const skippedSchedules = $derived(preview?.skippedSchedules ?? []);
  const shownSkipped = $derived(skippedSchedules.slice(0, 5));
</script>

{#if preview}
  <Dialog.Root bind:open={gnucashImportDialog.open}>
    <Dialog.Content
      class="max-h-[90vh] overflow-y-auto sm:max-w-lg"
      escapeKeydownBehavior={gnucashImportDialog.importing ? 'ignore' : 'close'}
      interactOutsideBehavior={gnucashImportDialog.importing ? 'ignore' : 'close'}
      showCloseButton={!gnucashImportDialog.importing}
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

      {#if scheduleCount > 0}
        <p class="text-sm text-muted-foreground">
          {scheduleCount} scheduled
          {scheduleCount === 1 ? 'transaction' : 'transactions'}
          {preview.schedulesAdded > 0 && preview.schedulesUpdated > 0
            ? `(${preview.schedulesAdded} added, ${preview.schedulesUpdated} updated)`
            : preview.schedulesAdded > 0
              ? 'added'
              : 'updated'}
        </p>
      {/if}

      {#if skippedSchedules.length > 0}
        <div class="rounded-md border p-3 text-sm">
          <p>
            {skippedSchedules.length} scheduled
            {skippedSchedules.length === 1 ? 'transaction' : 'transactions'} can’t be imported.
          </p>
          <ul class="mt-2 space-y-1 text-muted-foreground">
            {#each shownSkipped as skipped, i (i)}
              <li>{skipped.name}: {skipped.reason}</li>
            {/each}
            {#if skippedSchedules.length > shownSkipped.length}
              <li>and {skippedSchedules.length - shownSkipped.length} more</li>
            {/if}
          </ul>
        </div>
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
          disabled={gnucashImportDialog.importing}
        >
          Cancel
        </Button>
        <Button
          variant={hasRemovals && deleteOrphans ? 'destructive' : 'default'}
          disabled={gnucashImportDialog.importing}
          onclick={() => void confirmGnucashImport(hasRemovals ? deleteOrphans : true)}
        >
          {#if gnucashImportDialog.importing}
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
