<script lang="ts">
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
    Trash2
  } from '@lucide/svelte';
  import {
    addAttachments,
    deleteAttachment,
    exportAttachment,
    fetchAttachments,
    openAttachment,
    type AttachmentInfo
  } from '$lib/tauri';
  import { errMsg } from '$lib/utils/error';
  import { formatBytes, formatDate } from '$lib/utils/format.svelte';

  let { txGuid }: { txGuid: string } = $props();

  /** Mirrors `ALLOWED_TYPES` in `src-tauri/src/attachments.rs`; the backend re-validates. */
  const ATTACHMENT_EXTENSIONS = ['pdf', 'png', 'jpg', 'jpeg', 'gif', 'webp'];
  const MAX_ATTACHMENTS = 10;

  const res = useAsyncData({
    enabled: () => !!txGuid,
    fetch: () => fetchAttachments(txGuid),
    label: 'Failed to load attachments'
  });

  let adding = $state(false);
  let deleting = $state(false);
  let openBusy = $state(false);
  let saveBusy = $state(false);
  let confirmDeleteGuid = $state<string | null>(null);

  // Webview-level OS drop; anywhere on the page attaches to this transaction.
  const drop = useFileDrop((paths) => {
    void addPaths(paths);
  });

  let files = $derived(res.data ?? []);

  function isImage(item: AttachmentInfo): boolean {
    return item.mimeType.startsWith('image/');
  }

  async function pickFiles() {
    let selected: string | string[] | null;
    try {
      selected = await open({
        multiple: true,
        filters: [{ name: 'Documents', extensions: ATTACHMENT_EXTENSIONS }]
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
      toast.success(added.length === 1 ? 'File attached' : `${added.length} files attached`);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to add attachment'));
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
      toast.error(errMsg(e, 'Failed to open attachment'));
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
      toast.error(errMsg(e, 'Failed to open save dialog'));
      return;
    }
    if (!target) return;
    saveBusy = true;
    try {
      await exportAttachment(item.guid, target);
      toast.success(`Saved ${await basename(target)}`);
    } catch (e: unknown) {
      toast.error(errMsg(e, 'Failed to save attachment'));
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
      toast.error(errMsg(e, 'Failed to delete attachment'));
    } finally {
      deleting = false;
    }
  }
</script>

<Card.Root class="relative overflow-hidden">
  <IconCardHeader
    title="Attachments"
    description="PDF or image files stored inside this book (up to {MAX_ATTACHMENTS})."
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
    {#if res.loading && !res.data}
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
                title="Open file"
                onclick={() => handleOpen(item)}
                disabled={openBusy || saveBusy || deleting}
              >
                <ExternalLink aria-hidden="true" class="size-4" />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Save file"
                title="Save file"
                onclick={() => handleDownload(item)}
                disabled={openBusy || saveBusy || deleting}
              >
                <Download aria-hidden="true" class="size-4" />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Delete attachment"
                title="Delete attachment"
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
