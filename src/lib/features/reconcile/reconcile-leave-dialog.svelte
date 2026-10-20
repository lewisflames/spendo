<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';

  let {
    open = $bindable(),
    closeRequested,
    saving,
    onStay,
    onLeave,
    onSaveAndLeave
  }: {
    open: boolean;
    /** True when the window close button asked, not an in-app navigation. */
    closeRequested: boolean;
    saving: boolean;
    onStay: () => void;
    onLeave: () => void;
    onSaveAndLeave: () => void;
  } = $props();
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="sm:max-w-lg">
    <Dialog.Header>
      <Dialog.Title>{closeRequested ? 'Save before closing?' : 'Save before you go?'}</Dialog.Title>
      <Dialog.Description>
        {closeRequested
          ? 'You have unsaved cleared marks for this statement. Closing without saving loses them.'
          : 'You have unsaved cleared marks for this statement. Saving keeps them for next time.'}
      </Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button variant="ghost" onclick={onStay}>Stay</Button>
      <Button variant="outline" onclick={onLeave}>
        {closeRequested ? 'Close without saving' : 'Leave without saving'}
      </Button>
      <Button onclick={onSaveAndLeave} disabled={saving}>
        {saving ? 'Saving…' : closeRequested ? 'Save and close' : 'Save and leave'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
