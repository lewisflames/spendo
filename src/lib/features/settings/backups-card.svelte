<script lang="ts">
  import { tooltip } from '$lib/attachments/tooltip';
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Switch } from '$lib/components/ui/switch';
  import * as Dialog from '$lib/components/ui/dialog';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import SettingSelectRow from './setting-select-row.svelte';
  import { ArchiveRestore, FolderOpen, History, Trash2 } from '@lucide/svelte';
  import { join } from '@tauri-apps/api/path';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { activeBookPath, dbSession } from '$lib/stores/db.svelte';
  import { bookSettings, saveBookSetting } from '$lib/stores/book-settings.svelte';
  import { restoreBackupFile } from '$lib/features/book/book-actions';
  import {
    createBackup,
    deleteBackup,
    fetchBackups,
    type BackupEntry,
    type BackupReason
  } from '$lib/tauri';
  import { formatBytes, formatDate } from '$lib/utils/format.svelte';
  import { errMsg } from '$lib/utils/error';
  import { toast } from '$lib/components/ui/sonner';

  const REASON_LABEL: Record<BackupReason, string> = {
    daily: 'Daily',
    manual: 'Manual',
    beforeImport: 'Before import',
    beforeClear: 'Before clearing data',
    beforeRestore: 'Before restore'
  };

  const list = useAsyncData({
    enabled: () => !!activeBookPath.value,
    // Read the path so a book switch refetches.
    fetch: () => (void activeBookPath.value, fetchBackups()),
    label: 'We couldn’t load your backups'
  });

  const backups = $derived(list.data?.backups ?? []);
  const bookOpen = $derived(!!dbSession.value && !!activeBookPath.value);

  let busy = $state(false);
  let savingSetting = $state(false);
  let pending = $state<{ mode: 'restore' | 'delete'; entry: BackupEntry } | null>(null);
  let dialogOpen = $state(false);

  function when(entry: BackupEntry): string {
    return formatDate(entry.createdAt, { dateStyle: 'medium', timeStyle: 'short' });
  }

  async function handleAutoBackup(on: boolean) {
    if (!bookOpen || savingSetting) return;
    savingSetting = true;
    try {
      await saveBookSetting('autoBackup', on ? 'daily' : 'off');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t save that setting'));
    } finally {
      savingSetting = false;
    }
  }

  async function handleBackupNow() {
    busy = true;
    try {
      await createBackup();
      list.reload();
      toast.success('Backup saved');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t back up the book'));
    } finally {
      busy = false;
    }
  }

  async function handleShowFolder() {
    const folder = list.data?.folder;
    const newest = backups[0];
    if (!folder || !newest) return;
    try {
      await revealItemInDir(await join(folder, newest.fileName));
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t open that folder'));
    }
  }

  function ask(mode: 'restore' | 'delete', entry: BackupEntry) {
    pending = { mode, entry };
    dialogOpen = true;
  }

  async function confirm() {
    if (!pending) return;
    const { mode, entry } = pending;
    busy = true;
    try {
      if (mode === 'restore') {
        dialogOpen = false;
        await restoreBackupFile(entry.fileName);
      } else {
        await deleteBackup(entry.fileName);
        dialogOpen = false;
        toast.success('Backup deleted');
      }
      list.reload();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t delete that backup'));
    } finally {
      busy = false;
    }
  }
</script>

<Card.Root>
  <IconCardHeader title="Backups" description="Snapshots of this book, kept on this computer">
    {#snippet icon()}
      <History aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
    {#snippet action()}
      <Button variant="outline" size="sm" disabled={!bookOpen || busy} onclick={handleBackupNow}>
        Back up now
      </Button>
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-4">
    <SettingSelectRow
      label="Daily backups"
      description="One snapshot a day. Imports, clearing data and restores always take one first"
    >
      <Switch
        checked={bookSettings.value.autoBackup === 'daily'}
        onCheckedChange={(v) => void handleAutoBackup(v)}
        disabled={!bookOpen || savingSetting}
        aria-label="Take a daily backup"
      />
    </SettingSelectRow>

    {#if !bookOpen}
      <p class="text-xs text-muted-foreground">Open a book to see its backups.</p>
    {:else if list.error}
      <p class="text-xs text-destructive">{list.error}</p>
    {:else if list.data && backups.length === 0}
      <p class="text-xs text-muted-foreground">
        No backups yet. One is taken each day you use the book, and before every import.
      </p>
    {:else if backups.length > 0}
      <ul class="flex max-h-80 flex-col divide-y overflow-y-auto rounded-md border">
        {#each backups as entry (entry.fileName)}
          <li class="flex items-center gap-3 px-3 py-2">
            <div class="flex min-w-0 flex-1 flex-col">
              <span class="text-sm tabular-nums">{when(entry)}</span>
              <span class="text-xs text-muted-foreground">
                {REASON_LABEL[entry.reason]} &middot; {formatBytes(entry.byteSize)}
              </span>
            </div>
            <Button
              variant="outline"
              size="sm"
              disabled={busy}
              onclick={() => ask('restore', entry)}
            >
              <ArchiveRestore aria-hidden="true" class="size-4" />
              Restore
            </Button>
            <Button
              variant="ghost"
              size="icon"
              disabled={busy}
              onclick={() => ask('delete', entry)}
              aria-label="Delete backup"
              {@attach tooltip('Delete backup')}
            >
              <Trash2 aria-hidden="true" class="size-4" />
            </Button>
          </li>
        {/each}
      </ul>
      <div class="flex flex-wrap items-center justify-between gap-2">
        <span class="text-xs text-muted-foreground">
          We keep 7 daily and 10 safety backups. Ones you make stay until you delete them.
        </span>
        <Button variant="ghost" size="sm" onclick={handleShowFolder}>
          <FolderOpen aria-hidden="true" class="size-4" />
          Show in folder
        </Button>
      </div>
    {/if}
  </Card.Content>
</Card.Root>

<Dialog.Root bind:open={dialogOpen}>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-lg">
    {#if pending?.mode === 'restore'}
      <Dialog.Header>
        <Dialog.Title>Restore this backup?</Dialog.Title>
        <Dialog.Description>
          Your book goes back to how it was on {when(pending.entry)}. We save a backup of how it is
          now first, so you can change your mind.
        </Dialog.Description>
      </Dialog.Header>
    {:else if pending}
      <Dialog.Header>
        <Dialog.Title>Delete this backup?</Dialog.Title>
        <Dialog.Description>
          The snapshot from {when(pending.entry)} is removed from this computer. Your book isn’t touched.
        </Dialog.Description>
      </Dialog.Header>
    {/if}
    <Dialog.Footer>
      <Button variant="outline" onclick={() => (dialogOpen = false)} disabled={busy}>Cancel</Button>
      {#if pending?.mode === 'restore'}
        <Button onclick={confirm} disabled={busy}>Restore backup</Button>
      {:else}
        <Button variant="destructive" onclick={confirm} disabled={busy}>Delete backup</Button>
      {/if}
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
