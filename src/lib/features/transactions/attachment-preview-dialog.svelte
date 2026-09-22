<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { toast } from '$lib/components/ui/sonner';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { Download, ExternalLink, FileText, LoaderCircle, Trash2 } from '@lucide/svelte';
  import { save } from '@tauri-apps/plugin-dialog';
  import { basename } from '@tauri-apps/api/path';
  import {
    deleteAttachment,
    exportAttachment,
    fetchAttachmentData,
    openAttachment,
    type AttachmentInfo
  } from '$lib/tauri';
  import { useDialogReset } from '$lib/hooks/use-dialog-reset.svelte';
  import { errMsg } from '$lib/utils/error';
  import { formatBytes, formatDate } from '$lib/utils/format.svelte';

  let {
    open = $bindable(false),
    attachment,
    onDeleted,
    onError
  }: {
    open: boolean;
    /** The attachment to preview; read when the dialog opens. */
    attachment: AttachmentInfo | null;
    onDeleted?: () => void;
    onError?: (msg: string) => void;
  } = $props();

  let dataUrl = $state<string | null>(null);
  let loading = $state(false);
  let loadError = $state('');
  let busy = $state(false);
  let confirmDelete = $state(false);

  // Images load their bytes for the inline preview; PDFs and anything else are
  // opened in the OS viewer instead (WebViews have no reliable inline PDF view).
  useDialogReset(() => open, syncFromAttachment);

  function syncFromAttachment() {
    dataUrl = null;
    loadError = '';
    busy = false;
    confirmDelete = false;
    const current = attachment;
    if (!current || !current.mimeType.startsWith('image/')) {
      loading = false;
      return;
    }
    loading = true;
    fetchAttachmentData(current.guid)
      .then((data) => {
        // Drop a stale response if the dialog moved to another attachment.
        if (attachment?.guid === current.guid) dataUrl = data.dataUrl;
      })
      .catch((e: unknown) => {
        if (attachment?.guid === current.guid) {
          loadError = errMsg(e, 'Failed to load preview');
        }
      })
      .finally(() => {
        if (attachment?.guid === current.guid) loading = false;
      });
  }

  async function handleOpen() {
    if (!attachment || busy) return;
    busy = true;
    try {
      await openAttachment(attachment.guid);
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to open attachment');
      toast.error(msg);
      onError?.(msg);
    } finally {
      busy = false;
    }
  }

  async function handleDownload() {
    if (!attachment || busy) return;
    let target: string | null;
    try {
      target = await save({ defaultPath: attachment.filename });
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to open save dialog'));
      return;
    }
    if (!target) return;
    busy = true;
    try {
      await exportAttachment(attachment.guid, target);
      toast.success(`Saved ${await basename(target)}`);
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to save attachment');
      toast.error(msg);
      onError?.(msg);
    } finally {
      busy = false;
    }
  }

  async function handleDelete() {
    if (!attachment || busy) return;
    if (!confirmDelete) {
      confirmDelete = true;
      return;
    }
    busy = true;
    try {
      await deleteAttachment(attachment.guid);
      toast.success('Attachment deleted');
      open = false;
      onDeleted?.();
    } catch (e: unknown) {
      const msg = errMsg(e, 'Failed to delete attachment');
      toast.error(msg);
      onError?.(msg);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
    <Dialog.Header>
      <Dialog.Title class="truncate">{attachment?.filename ?? 'Attachment'}</Dialog.Title>
      <Dialog.Description>
        {#if attachment}
          {attachment.mimeType} &middot; {formatBytes(attachment.byteSize)} &middot; added {formatDate(
            attachment.createdAt
          )}
        {/if}
      </Dialog.Description>
    </Dialog.Header>

    {#if loading}
      <div class="flex items-center justify-center py-12">
        <LoaderCircle class="size-5 animate-spin text-muted-foreground" aria-hidden="true" />
        <span class="sr-only">Loading preview</span>
      </div>
    {:else if loadError}
      <ErrorCard message={loadError} onRetry={syncFromAttachment} />
    {:else if dataUrl}
      <div
        class="flex max-h-[60vh] items-center justify-center overflow-hidden rounded-lg border bg-muted/30 p-2"
      >
        <img
          src={dataUrl}
          alt={attachment?.filename ?? 'Attachment preview'}
          class="max-h-[56vh] w-auto object-contain"
        />
      </div>
    {:else if attachment}
      <div
        class="flex flex-col items-center gap-2 rounded-lg border border-dashed px-4 py-10 text-center"
      >
        <FileText class="size-8 text-muted-foreground" aria-hidden="true" />
        <p class="text-sm text-muted-foreground">
          No inline preview for this file type — open it in your default app.
        </p>
      </div>
    {/if}

    {#if confirmDelete}
      <p role="alert" class="text-sm text-destructive">
        Delete this attachment? This cannot be undone.
      </p>
    {/if}

    <Dialog.Footer class="sm:justify-between">
      <Button
        variant={confirmDelete ? 'destructive' : 'ghost'}
        size="sm"
        onclick={handleDelete}
        disabled={busy || !attachment}
      >
        <Trash2 aria-hidden="true" class="size-4" />
        {confirmDelete ? 'Confirm delete' : 'Delete'}
      </Button>
      <div class="flex gap-2">
        <Button variant="outline" size="sm" onclick={handleOpen} disabled={busy || !attachment}>
          <ExternalLink aria-hidden="true" class="size-4" />
          Open
        </Button>
        <Button variant="outline" size="sm" onclick={handleDownload} disabled={busy || !attachment}>
          <Download aria-hidden="true" class="size-4" />
          Save as…
        </Button>
      </div>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
