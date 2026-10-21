<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { plural } from '$lib/utils/format.svelte';

  let {
    open = $bindable(),
    attachmentCount,
    lockedSplitCount,
    deleting,
    onConfirm
  }: {
    open: boolean;
    attachmentCount: number;
    /** Cleared or reconciled splits; deleting drops them from reconciliation history. */
    lockedSplitCount: number;
    deleting: boolean;
    onConfirm: () => void;
  } = $props();
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Delete this one?</Dialog.Title>
      <Dialog.Description>
        It’ll be gone for good, splits and all.
        {#if attachmentCount > 0}
          Its {plural(attachmentCount, 'attachment')} are deleted permanently too.
        {/if}
      </Dialog.Description>
    </Dialog.Header>
    {#if lockedSplitCount > 0}
      <p
        role="note"
        class="rounded-lg border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm text-amber-700 dark:text-amber-300"
      >
        {plural(lockedSplitCount, 'split')} here {lockedSplitCount === 1 ? 'is' : 'are'} cleared or reconciled.
        Deleting removes them from the account’s reconciliation history.
      </p>
    {/if}
    <Dialog.Footer>
      <Button variant="outline" onclick={() => (open = false)} disabled={deleting}>Cancel</Button>
      <Button variant="destructive" onclick={onConfirm} disabled={deleting}>
        {deleting ? 'Deleting…' : 'Yes, delete'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
