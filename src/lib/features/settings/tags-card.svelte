<script lang="ts">
  import { resolve } from '$app/paths';
  import { tooltip } from '$lib/attachments/tooltip';
  import * as Card from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import ErrorCard from '$lib/components/shared/error-card.svelte';
  import { deleteTag, fetchTags, renameTag, type TagSummary } from '$lib/tauri';
  import { MAX_TAG_LEN, normalizeTagName, tagNameError } from '$lib/features/tags/tag-utils';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { errMsg } from '$lib/utils/error';
  import { formatDate, plural } from '$lib/utils/format.svelte';
  import { toast } from '$lib/components/ui/sonner';
  import { Check, Pencil, Tags, Trash2, X } from '@lucide/svelte';

  const tagsRes = useAsyncData({
    enabled: () => true,
    fetch: fetchTags,
    label: 'We couldn’t load your tags'
  });
  const tags = $derived(tagsRes.data ?? []);

  let editingGuid = $state<string | null>(null);
  let draft = $state('');
  let busyGuid = $state<string | null>(null);
  let confirmDeleteGuid = $state<string | null>(null);

  let draftError = $derived(editingGuid ? tagNameError(normalizeTagName(draft)) : null);
  // Renaming onto another tag's name merges the two; say so before saving.
  let mergeTarget = $derived.by(() => {
    if (!editingGuid) return null;
    const lower = normalizeTagName(draft).toLowerCase();
    return tags.find((t) => t.guid !== editingGuid && t.name.toLowerCase() === lower) ?? null;
  });

  const listHref = (guid: string) => `${resolve('/transactions')}?tag=${encodeURIComponent(guid)}`;

  function startEdit(tag: TagSummary) {
    editingGuid = tag.guid;
    draft = tag.name;
    confirmDeleteGuid = null;
  }

  async function saveEdit(tag: TagSummary) {
    if (busyGuid || draftError) return;
    const name = normalizeTagName(draft);
    if (name === tag.name) {
      editingGuid = null;
      return;
    }
    busyGuid = tag.guid;
    try {
      await renameTag(tag.guid, name);
      toast.success(mergeTarget ? `Merged into ${mergeTarget.name}` : 'Tag renamed');
      editingGuid = null;
      tagsRes.reload();
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t rename this tag'));
    } finally {
      busyGuid = null;
    }
  }

  async function handleDelete(tag: TagSummary) {
    if (busyGuid) return;
    if (confirmDeleteGuid !== tag.guid) {
      confirmDeleteGuid = tag.guid;
      return;
    }
    busyGuid = tag.guid;
    try {
      await deleteTag(tag.guid);
      confirmDeleteGuid = null;
      tagsRes.reload();
      toast.success('Tag deleted');
    } catch (e: unknown) {
      toast.error(errMsg(e, 'We couldn’t delete this tag'));
    } finally {
      busyGuid = null;
    }
  }
</script>

<Card.Root>
  <IconCardHeader
    title="Tags"
    description="Labels on transactions, added from the transaction form or bulk edit. Rename one onto another’s name to merge them."
  >
    {#snippet icon()}
      <Tags aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-3">
    {#if tagsRes.loading && !tagsRes.data}
      <p class="py-2 text-sm text-muted-foreground">Finding your tags…</p>
    {:else if tagsRes.error}
      <ErrorCard message={tagsRes.error} onRetry={() => tagsRes.reload()} />
    {:else if tags.length === 0}
      <p class="text-sm text-muted-foreground">
        No tags yet. Add one to any transaction, like “Summer trip” or “Deductible”.
      </p>
    {:else}
      <p class="text-xs text-muted-foreground">{plural(tags.length, 'tag')}</p>
      <ul class="flex flex-col gap-1.5">
        {#each tags as tag (tag.guid)}
          <li class="flex items-center gap-2 rounded-lg border px-3 py-2">
            {#if editingGuid === tag.guid}
              <form
                class="flex min-w-0 flex-1 flex-col gap-1"
                onsubmit={(e) => {
                  e.preventDefault();
                  void saveEdit(tag);
                }}
              >
                <div class="flex items-center gap-2">
                  <Input
                    bind:value={draft}
                    aria-label="Tag name"
                    maxlength={MAX_TAG_LEN + 10}
                    aria-invalid={draftError ? true : undefined}
                    class="h-7"
                    autofocus
                    onkeydown={(e) => {
                      if (e.key === 'Escape') editingGuid = null;
                    }}
                  />
                  <Button
                    type="submit"
                    variant="ghost"
                    size="icon-sm"
                    aria-label="Save tag name"
                    disabled={!!draftError || busyGuid !== null}
                  >
                    <Check aria-hidden="true" class="size-4" />
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label="Cancel renaming"
                    onclick={() => (editingGuid = null)}
                  >
                    <X aria-hidden="true" class="size-4" />
                  </Button>
                </div>
                {#if draftError}
                  <p role="alert" class="text-xs text-destructive">{draftError}</p>
                {:else if mergeTarget}
                  <p class="text-xs text-muted-foreground">
                    Saving merges this tag into {mergeTarget.name}.
                  </p>
                {/if}
              </form>
            {:else}
              <div class="flex min-w-0 flex-1 flex-col">
                <!-- eslint-disable svelte/no-navigation-without-resolve -- listHref resolves the path -->
                <a
                  href={listHref(tag.guid)}
                  class="truncate text-sm font-medium hover:underline focus-visible:outline-2 focus-visible:outline-ring"
                  {@attach tooltip(tag.name, { truncated: true })}
                >
                  {tag.name}
                </a>
                <!-- eslint-enable svelte/no-navigation-without-resolve -->
                <span class="text-xs text-muted-foreground">
                  {plural(tag.transactionCount, 'transaction')}{tag.lastUsed
                    ? ` · last on ${formatDate(tag.lastUsed)}`
                    : ''}
                </span>
              </div>
              <div class="flex shrink-0 items-center gap-0.5">
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Rename tag {tag.name}"
                  {@attach tooltip('Rename')}
                  disabled={busyGuid !== null}
                  onclick={() => startEdit(tag)}
                >
                  <Pencil aria-hidden="true" class="size-4" />
                </Button>
                {#if confirmDeleteGuid === tag.guid}
                  <Button
                    variant="destructive"
                    size="sm"
                    onclick={() => void handleDelete(tag)}
                    disabled={busyGuid !== null}
                  >
                    {busyGuid === tag.guid ? 'Deleting…' : 'Confirm delete'}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onclick={() => (confirmDeleteGuid = null)}
                    disabled={busyGuid !== null}
                  >
                    Cancel
                  </Button>
                {:else}
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    class="text-destructive hover:text-destructive"
                    aria-label="Delete tag {tag.name}"
                    {@attach tooltip('Delete. Transactions stay, just without this tag')}
                    disabled={busyGuid !== null}
                    onclick={() => void handleDelete(tag)}
                  >
                    <Trash2 aria-hidden="true" class="size-4" />
                  </Button>
                {/if}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </Card.Content>
</Card.Root>
