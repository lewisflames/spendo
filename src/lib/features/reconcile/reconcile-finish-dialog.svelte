<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';

  let {
    open = $bindable(),
    checkedCount,
    finishing,
    onConfirm
  }: {
    open: boolean;
    checkedCount: number;
    finishing: boolean;
    onConfirm: () => void;
  } = $props();
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Finish reconciliation?</Dialog.Title>
      <Dialog.Description>
        This saves your progress and marks the {checkedCount} ticked splits as reconciled. Editing a transaction
        later will ask before putting its splits back to New.
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button variant="outline" onclick={() => (open = false)} disabled={finishing}>Cancel</Button>
      <Button
        disabled={finishing}
        onclick={() => {
          open = false;
          onConfirm();
        }}
      >
        {finishing ? 'Finishing…' : 'Finish reconciliation'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
