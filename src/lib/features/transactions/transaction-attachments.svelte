<script lang="ts">
  import { tooltip } from '$lib/attachments/tooltip';
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useFileDrop } from '$lib/hooks/use-file-drop.svelte';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { basename } from '@tauri-apps/api/path';
  import {
    Download,
    ExternalLink,
    FileText,
    Image,
    LoaderCircle,
    Paperclip,
    Plus,
    Trash2,
    X
  } from '@lucide/svelte';
  import {
    addAttachments,
    deleteAttachment,
    exportAttachment,
    fetchAttachments,
    openAttachment,
    type AttachmentInfo
  } from '$lib/tauri';
  import {
    ATTACHMENT_EXTENSIONS,
    MAX_ATTACHMENTS,
    attachmentFileName,
    isImageAttachment,
    selectPendingAttachments
  } from '$lib/features/transactions/attachment-utils';
  import { errMsg } from '$lib/utils/error';
  import { formatBytes, formatDate } from '$lib/utils/format.svelte';

  let {
    txGuid = null,
    pendingPaths = $bindable<string[]>([])
  }: {
    txGuid?: string | null;
    /** Create mode: files held locally until the form saves the transaction. */
    pendingPaths?: string[];
  } = $props();

  const res = useAsyncData({
    enabled: () => !!txGuid,
    fetch: () => fetchAttachments(txGuid ?? ''),
    label: 'We couldn’t load the attachments'
  });

  let adding = $state(false);
  let deleting = $state(false);
  let openBusy = $state(false);
  let saveBusy = $state(false);
  let confirmDeleteGuid = $state<string | null>(null);

  // Webview-level OS drop; anywhere on the page attaches to this transaction.
  // In create mode files wait in `pendingPaths` until the form saves.
  const drop = useFileDrop((paths) => handlePicked(paths));

  let files = $derived(res.data ?? []);

  function isImage(item: AttachmentInfo): boolean {
    return item.mimeType.startsWith('image/');
  }

  async function pickFiles() {
    if (adding) return;
    let selected: string | string[] | null;
    try {
      selected = await open({
        multiple: true,
        filters: [{ name: 'Documents', extensions: ATTACHMENT_EXTENSIONS }]
      });
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t open the file picker'));
      return;
    }
    if (!selected) return;
    handlePicked(Array.isArray(selected) ? selected : [selected]);
  }

  function handlePicked(paths: string[]) {
    if (!paths.length) return;
    if (txGuid) {
      void addPaths(paths);
      return;
    }
    const result = selectPendingAttachments(pendingPaths, paths, MAX_ATTACHMENTS);
    if (result.added.length > 0) pendingPaths = [...pendingPaths, ...result.added];
    if (result.unsupported.length > 0) {
      toast.error(
        result.unsupported.length === 1
          ? `${attachmentFileName(result.unsupported[0])} isn’t a PDF or image, so it was skipped`
          : `${result.unsupported.length} files aren’t PDFs or images, so they were skipped`
      );
    }
    if (result.overflow > 0) {
      toast.error(`Up to ${MAX_ATTACHMENTS} attachments per transaction`);
    }
    if (result.added.length > 0) {
      toast.success(
        result.added.length === 1
          ? 'Attaches when you save'
          : `${result.added.length} files ready. They’ll attach when you save`
      );
    }
  }

  function removePending(path: string) {
    pendingPaths = pendingPaths.filter((p) => p !== path);
  }

  async function addPaths(paths: string[]) {
    if (!txGuid || !paths.length || adding) return;
    adding = true;
    try {
      const added = await addAttachments({ txGuid, paths });
      res.reload();
      toast.success(
        added.length === 1 ? 'Attached' : `${added.length} files tucked into your book`
      );
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t attach that file'));
    } finally {
      adding = false;
    }
  }

  async function handleOpen(item: AttachmentInfo) {
    if (openBusy || saveBusy) return;
    openBusy = true;
    try {
      await openAttachment(item.guid);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t open that file'));
    } finally {
      openBusy = false;
    }
  }

  async function handleDownload(item: AttachmentInfo) {
    if (openBusy || saveBusy) return;
    let target: string | null;
    try {
      target = await save({ defaultPath: item.filename });
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t open the save dialog'));
      return;
    }
    if (!target) return;
    saveBusy = true;
    try {
      await exportAttachment(item.guid, target);
      toast.success(`Saved ${await basename(target)}`);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t save that file'));
    } finally {
      saveBusy = false;
    }
  }

  async function handleDelete(item: AttachmentInfo) {
    if (deleting) return;
    if (confirmDeleteGuid !== item.guid) {
      confirmDeleteGuid = item.guid;
      return;
    }
    deleting = true;
    try {
      await deleteAttachment(item.guid);
      confirmDeleteGuid = null;
      res.reload();
      toast.success('Attachment deleted');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t delete that file'));
    } finally {
      deleting = false;
    }
  }
</script>

<Card.Root class="relative overflow-hidden">
  <IconCardHeader
    title="Attachments"
    description="Receipts and images tucked inside this book (up to {MAX_ATTACHMENTS})."
  >
    {#snippet icon()}
      <Paperclip aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
    {#snippet action()}
      <Button variant="outline" size="sm" onclick={pickFiles} disabled={adding}>
        {#if adding}
          <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
          Adding…
        {:else}
          <Plus aria-hidden="true" class="size-4" />
          Add files
        {/if}
      </Button>
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-3">
    {#if !txGuid}
      {#if pendingPaths.length > 0}
        <ul class="flex flex-col gap-1.5">
          {#each pendingPaths as path (path)}
            <li class="flex items-center gap-2 rounded-lg border px-3 py-2">
              <span class="flex min-w-0 flex-1 items-center gap-2">
                {#if isImageAttachment(path)}
                  <Image aria-hidden="true" class="size-4 shrink-0 text-muted-foreground" />
                {:else}
                  <FileText aria-hidden="true" class="size-4 shrink-0 text-muted-foreground" />
                {/if}
                <span class="min-w-0">
                  <span class="block truncate text-sm font-medium">{attachmentFileName(path)}</span>
                  <span class="block text-xs text-muted-foreground">Attaches when you save</span>
                </span>
              </span>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Remove file"
                {@attach tooltip('Remove file')}
                onclick={() => removePending(path)}
                disabled={adding}
              >
                <X aria-hidden="true" class="size-4" />
              </Button>
            </li>
          {/each}
        </ul>
      {:else}
        <div class="rounded-lg border border-dashed px-4 py-6 text-center">
          <p class="text-sm text-muted-foreground">
            Drop files here, or use Add files. They’ll attach when you save (PDF or images, max 10
            MB each, up to {MAX_ATTACHMENTS}).
          </p>
        </div>
      {/if}
    {:else if res.loading && !res.data}
      <p class="py-2 text-sm text-muted-foreground">Loading attachments…</p>
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {:else if files.length > 0}
      <ul class="flex flex-col gap-1.5">
        {#each files as item (item.guid)}
          <li class="flex items-center gap-2 rounded-lg border px-3 py-2">
            <span class="flex min-w-0 flex-1 items-center gap-2">
              {#if isImage(item)}
                <Image aria-hidden="true" class="size-4 shrink-0 text-muted-foreground" />
              {:else}
                <FileText aria-hidden="true" class="size-4 shrink-0 text-muted-foreground" />
              {/if}
              <span class="min-w-0">
                <span class="block truncate text-sm font-medium">{item.filename}</span>
                <span class="block text-xs text-muted-foreground">
                  {formatBytes(item.byteSize)} &middot; {formatDate(item.createdAt)}
                </span>
              </span>
            </span>
            {#if confirmDeleteGuid === item.guid}
              <span class="text-xs text-destructive">Delete?</span>
              <Button
                variant="destructive"
                size="sm"
                onclick={() => handleDelete(item)}
                disabled={deleting}
              >
                Delete
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onclick={() => (confirmDeleteGuid = null)}
                disabled={deleting}
              >
                Cancel
              </Button>
            {:else}
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Open file"
                {@attach tooltip('Open file')}
                onclick={() => handleOpen(item)}
                disabled={openBusy || saveBusy || deleting}
              >
                <ExternalLink aria-hidden="true" class="size-4" />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Save file"
                {@attach tooltip('Save file')}
                onclick={() => handleDownload(item)}
                disabled={openBusy || saveBusy || deleting}
              >
                <Download aria-hidden="true" class="size-4" />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Delete attachment"
                {@attach tooltip('Delete attachment')}
                onclick={() => handleDelete(item)}
                disabled={openBusy || saveBusy || deleting}
              >
                <Trash2 aria-hidden="true" class="size-4" />
              </Button>
            {/if}
          </li>
        {/each}
      </ul>
    {:else}
      <div class="rounded-lg border border-dashed px-4 py-6 text-center">
        <p class="text-sm text-muted-foreground">
          Drop files here, or use Add files (PDF or images, max 10 MB each, up to {MAX_ATTACHMENTS}).
        </p>
      </div>
    {/if}
  </Card.Content>

  {#if drop.dragging && !adding}
    <div
      class="pointer-events-none absolute inset-0 z-10 flex items-center justify-center rounded-xl border-2 border-dashed border-primary bg-background/85"
    >
      <p class="text-sm font-medium">Drop files to attach</p>
    </div>
  {/if}
</Card.Root>
