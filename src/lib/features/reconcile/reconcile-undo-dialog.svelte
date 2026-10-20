<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { formatDate, plural } from '$lib/utils/format.svelte';

  let {
    open = $bindable(),
    lastReconciledCount,
    lastReconciledDate,
    undoing,
    onConfirm
  }: {
    open: boolean;
    lastReconciledCount: number;
    lastReconciledDate: string;
    undoing: boolean;
    onConfirm: () => void;
  } = $props();
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Undo last reconciliation?</Dialog.Title>
      <Dialog.Description>
        This returns {plural(lastReconciledCount, 'split')} from your last statement (through
        {formatDate(lastReconciledDate)}) to Cleared, so you can tick them off and finish again.
        Earlier reconciled statements are untouched.
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button variant="outline" onclick={() => (open = false)} disabled={undoing}>Cancel</Button>
      <Button variant="destructive" onclick={onConfirm} disabled={undoing}>
        {undoing ? 'Reopening…' : 'Undo reconciliation'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
