<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import SettingSelectRow from './setting-select-row.svelte';
  import { Trash2, RotateCcw } from '@lucide/svelte';
  import { dbSession, refreshSession } from '$lib/stores/db.svelte';
  import { resetBookSettings } from '$lib/stores/book-settings.svelte';
  import { clearAppDb } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';

  let { onResetPreferences }: { onResetPreferences: () => void } = $props();

  let clearDialogOpen = $state(false);
  let resetDialogOpen = $state(false);
  let clearing = $state(false);

  async function handleClearDb() {
    clearing = true;
    try {
      await clearAppDb();
      // Stay in the book: the shell now shows its empty states, so the user
      // can import fresh data without hunting for the file again.
      refreshSession();
      resetBookSettings();
      clearDialogOpen = false;
      toast.success('Book cleared');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t clear your data'));
    } finally {
      clearing = false;
    }
  }

  function handleReset() {
    resetDialogOpen = false;
    onResetPreferences();
  }
</script>

<Card.Root class="border-destructive/30">
  <IconCardHeader
    title="Danger zone"
    description="Be careful here. Clearing data takes a backup first; nothing else can be undone."
  >
    {#snippet icon()}
      <Trash2 aria-hidden="true" class="size-4 text-destructive" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col items-start gap-4">
    <SettingSelectRow
      label="Clear all data"
      description="Erase accounts, transactions, schedules, budgets, prices, receipts, import rules and targets from this book"
      class="w-full"
    >
      <Dialog.Root bind:open={clearDialogOpen}>
        <Button
          variant="destructive"
          disabled={!dbSession.value || clearing}
          onclick={() => (clearDialogOpen = true)}
        >
          Clear all data
        </Button>
        <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
          <Dialog.Header>
            <Dialog.Title>Clear all data?</Dialog.Title>
            <Dialog.Description>
              This wipes accounts, transactions, schedules, budgets, prices, receipts, import rules
              and portfolio targets. We save a backup first, so you can restore it from Backups.
              Your theme and display choices stay put.
            </Dialog.Description>
          </Dialog.Header>
          <Dialog.Footer>
            <Button variant="outline" onclick={() => (clearDialogOpen = false)} disabled={clearing}>
              Cancel
            </Button>
            <Button variant="destructive" onclick={handleClearDb} disabled={clearing}>
              {clearing ? 'Clearing…' : 'Yes, clear everything'}
            </Button>
          </Dialog.Footer>
        </Dialog.Content>
      </Dialog.Root>
    </SettingSelectRow>
    {#if !dbSession.value}
      <p class="text-xs text-muted-foreground">Nothing in here yet, so nothing to clear.</p>
    {/if}
    <SettingSelectRow
      label="Reset preferences"
      description="Back to the original theme, accent, density, locale and list setup"
      class="w-full"
    >
      <Dialog.Root bind:open={resetDialogOpen}>
        <Button variant="outline" onclick={() => (resetDialogOpen = true)}>
          <RotateCcw aria-hidden="true" class="size-4" />
          Reset to defaults
        </Button>
        <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
          <Dialog.Header>
            <Dialog.Title>Reset preferences?</Dialog.Title>
            <Dialog.Description>
              This puts your look and feel back to defaults. Your money data stays exactly as it is.
            </Dialog.Description>
          </Dialog.Header>
          <Dialog.Footer>
            <Button variant="outline" onclick={() => (resetDialogOpen = false)}>Cancel</Button>
            <Button variant="outline" onclick={handleReset}>Reset preferences</Button>
          </Dialog.Footer>
        </Dialog.Content>
      </Dialog.Root>
    </SettingSelectRow>
  </Card.Content>
</Card.Root>
