<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import SettingSelectRow from './setting-select-row.svelte';
  import { Trash2, RotateCcw } from '@lucide/svelte';
  import { dbSession, clearSession } from '$lib/stores/db.svelte';
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
      clearSession();
      resetBookSettings();
      clearDialogOpen = false;
      toast.success('Database cleared');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to clear database'));
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
  <IconCardHeader title="Danger zone" description="Destructive actions. These cannot be undone.">
    {#snippet icon()}
      <Trash2 aria-hidden="true" class="size-4 text-destructive" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col items-start gap-4">
    <SettingSelectRow
      label="Clear all data"
      description="Delete accounts, transactions, scheduled transactions, budgets, prices and history from this book"
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
        <Dialog.Content>
          <Dialog.Header>
            <Dialog.Title>Clear all data?</Dialog.Title>
            <Dialog.Description>
              This will permanently delete all accounts, transactions, scheduled transactions,
              budgets, prices, symbol overrides and changes history. Your appearance preferences
              will be preserved.
            </Dialog.Description>
          </Dialog.Header>
          <Dialog.Footer>
            <Button variant="outline" onclick={() => (clearDialogOpen = false)} disabled={clearing}>
              Cancel
            </Button>
            <Button variant="destructive" onclick={handleClearDb} disabled={clearing}>
              {clearing ? 'Clearing...' : 'Yes, clear everything'}
            </Button>
          </Dialog.Footer>
        </Dialog.Content>
      </Dialog.Root>
    </SettingSelectRow>
    {#if !dbSession.value}
      <p class="text-xs text-muted-foreground">No data imported yet — nothing to clear.</p>
    {/if}
    <SettingSelectRow
      label="Reset preferences"
      description="Restore theme, density, locale and list settings to defaults"
      class="w-full"
    >
      <Dialog.Root bind:open={resetDialogOpen}>
        <Button variant="outline" onclick={() => (resetDialogOpen = true)}>
          <RotateCcw aria-hidden="true" class="size-4" />
          Reset to defaults
        </Button>
        <Dialog.Content>
          <Dialog.Header>
            <Dialog.Title>Reset preferences?</Dialog.Title>
            <Dialog.Description>
              This restores all appearance and display settings to their defaults. Your book data is
              not touched.
            </Dialog.Description>
          </Dialog.Header>
          <Dialog.Footer>
            <Button variant="outline" onclick={() => (resetDialogOpen = false)}>Cancel</Button>
            <Button variant="destructive" onclick={handleReset}>Reset preferences</Button>
          </Dialog.Footer>
        </Dialog.Content>
      </Dialog.Root>
    </SettingSelectRow>
  </Card.Content>
</Card.Root>
