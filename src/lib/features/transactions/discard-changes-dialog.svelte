<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';

  let {
    open = $bindable(),
    onResolve
  }: {
    open: boolean;
    /** `true` leaves and drops the edits; `false` (also on dismiss) keeps editing. */
    onResolve: (leave: boolean) => void;
  } = $props();
</script>

<Dialog.Root
  bind:open
  onOpenChange={(next) => {
    if (!next) onResolve(false);
  }}
>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>Lose your changes?</Dialog.Title>
      <Dialog.Description
        >You’ve got unsaved edits. Leave now and you’ll lose them.</Dialog.Description
      >
    </Dialog.Header>
    <Dialog.Footer>
      <Button variant="outline" onclick={() => onResolve(false)}>Keep editing</Button>
      <Button variant="destructive" onclick={() => onResolve(true)}>Discard changes</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
