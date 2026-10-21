<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';

  let {
    open = $bindable(),
    saving,
    onConfirm
  }: {
    open: boolean;
    saving: boolean;
    onConfirm: () => void;
  } = $props();
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Save and unreconcile?</Dialog.Title>
      <Dialog.Description>
        Saving re-creates this transaction’s splits, so every cleared or reconciled split goes back
        to New. Reconciled splits also drop out of the account’s starting balance for its next
        reconciliation, and all of them show up again as candidates.
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button variant="outline" onclick={() => (open = false)} disabled={saving}>Cancel</Button>
      <Button
        onclick={() => {
          open = false;
          onConfirm();
        }}
        disabled={saving}
      >
        {saving ? 'Saving…' : 'Save anyway'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
