<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import AttachmentPreviewDialog from './attachment-preview-dialog.svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { useFileDrop } from '$lib/hooks/use-file-drop.svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    FileText,
    Image as ImageIcon,
    LoaderCircle,
    Paperclip,
    Plus,
    Trash2
  } from '@lucide/svelte';
  import {
    addAttachments,
    deleteAttachment,
    fetchAttachments,
    type AttachmentInfo
  } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { formatBytes, formatDate, plural } from '$lib/utils/format.svelte';

  let { txGuid }: { txGuid: string } = $props();

  /** Mirrors `ALLOWED_TYPES` in `src-tauri/src/attachments.rs`; the backend re-validates. */
  const ATTACHMENT_EXTENSIONS = ['png', 'jpg', 'jpeg', 'webp', 'gif', 'pdf'];

  const res = useAsyncData({
    enabled: () => !!txGuid,
    fetch: () => fetchAttachments(txGuid),
    label: 'Failed to load attachments'
  });

  let adding = $state(false);
  let deleting = $state(false);
  let confirmDeleteGuid = $state<string | null>(null);
  let preview = $state<AttachmentInfo | null>(null);
  let previewOpen = $state(false);

  // Webview-level OS drop; anywhere on the page attaches to this transaction.
  const drop = useFileDrop((paths) => {
    void addPaths(paths);
  });

  function isImage(attachment: AttachmentInfo): boolean {
    return attachment.mimeType.startsWith('image/');
  }

  function openPreview(attachment: AttachmentInfo) {
    preview = attachment;
    previewOpen = true;
  }

  async function pickFiles() {
    let selected: string | string[] | null;
    try {
      selected = await open({
        multiple: true,
        filters: [{ name: 'Receipts & documents', extensions: ATTACHMENT_EXTENSIONS }]
      });
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to open file dialog'));
      return;
    }
    if (!selected) return;
    await addPaths(Array.isArray(selected) ? selected : [selected]);
  }

  async function addPaths(paths: string[]) {
    if (!paths.length || adding) return;
    adding = true;
    try {
      const added = await addAttachments({ txGuid, paths });
      res.reload();
      toast.success(`Added ${plural(added.length, 'attachment')}`);
    } catch (e: unknown) {
      // One bad file fails the whole batch — the message names the file.
      toast.error(errMsg(e, 'Failed to add attachment'));
    } finally {
      adding = false;
    }
  }

  async function handleDelete(attachment: AttachmentInfo) {
    if (deleting) return;
    if (confirmDeleteGuid !== attachment.guid) {
      confirmDeleteGuid = attachment.guid;
      return;
    }
    deleting = true;
    try {
      await deleteAttachment(attachment.guid);
      confirmDeleteGuid = null;
      if (preview?.guid === attachment.guid) previewOpen = false;
      res.reload();
      toast.success('Attachment deleted');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to delete attachment'));
    } finally {
      deleting = false;
    }
  }
</script>

<Card.Root class="relative overflow-hidden">
  <IconCardHeader title="Attachments" description="Receipts and documents stored inside this book.">
    {#snippet icon()}
      <Paperclip aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-3">
    <div class="flex items-center justify-between gap-3">
      <p class="text-xs text-muted-foreground">
        {#if res.data && res.data.length > 0}
          {plural(res.data.length, 'file')}
        {:else}
          No files yet
        {/if}
      </p>
      <Button variant="outline" size="sm" onclick={pickFiles} disabled={adding}>
        {#if adding}
          <LoaderCircle aria-hidden="true" class="size-4 animate-spin" />
          Adding…
        {:else}
          <Plus aria-hidden="true" class="size-4" />
          Add attachment
        {/if}
      </Button>
    </div>

    {#if res.loading && !res.data}
      <p class="py-2 text-sm text-muted-foreground">Loading attachments…</p>
    {:else if res.error}
      <ErrorCard message={res.error} onRetry={() => res.reload()} />
    {:else if res.data && res.data.length > 0}
      <ul class="flex flex-col gap-1.5">
        {#each res.data as attachment (attachment.guid)}
          <li class="flex items-center gap-2 rounded-lg border px-3 py-2">
            <button
              type="button"
              class="flex min-w-0 flex-1 items-center gap-2 rounded-sm text-left focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              onclick={() => openPreview(attachment)}
            >
              {#if isImage(attachment)}
                <ImageIcon aria-hidden="true" class="size-4 shrink-0 text-muted-foreground" />
              {:else}
                <FileText aria-hidden="true" class="size-4 shrink-0 text-muted-foreground" />
              {/if}
              <span class="min-w-0">
                <span class="block truncate text-sm font-medium">{attachment.filename}</span>
                <span class="block text-xs text-muted-foreground">
                  {formatBytes(attachment.byteSize)} &middot; {formatDate(attachment.createdAt)}
                </span>
              </span>
            </button>
            {#if confirmDeleteGuid === attachment.guid}
              <span class="text-xs text-destructive">Delete?</span>
              <Button
                variant="destructive"
                size="sm"
                onclick={() => handleDelete(attachment)}
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
                aria-label="Delete attachment"
                title="Delete attachment"
                onclick={() => handleDelete(attachment)}
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
          Drop receipts here, or use Add attachment (PNG, JPG, WebP, GIF or PDF, max 10 MB).
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

<AttachmentPreviewDialog
  bind:open={previewOpen}
  attachment={preview}
  onDeleted={() => res.reload()}
/>
