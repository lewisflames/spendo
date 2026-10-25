<script lang="ts">
  import { resolve } from '$app/paths';
  import { Badge } from '$lib/components/ui/badge';
  import { cn } from '$lib/utils';
  import type { TagRef } from '$lib/tauri';

  /** A transaction's tags. Linked badges open the transactions list filtered to that tag. */
  let {
    tags,
    linked = true,
    class: className = undefined
  }: {
    tags: TagRef[];
    linked?: boolean;
    class?: string;
  } = $props();

  // Query strings can't go through `resolve()`.
  const listHref = (guid: string) => `${resolve('/transactions')}?tag=${encodeURIComponent(guid)}`;
</script>

{#if tags.length > 0}
  <span class={cn('flex flex-wrap items-center gap-1', className)}>
    {#each tags as tag (tag.guid)}
      {#if linked}
        <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
        <Badge variant="secondary" class="max-w-40 font-normal" href={listHref(tag.guid)}>
          <span class="truncate">{tag.name}</span>
        </Badge>
      {:else}
        <Badge variant="secondary" class="max-w-40 font-normal">
          <span class="truncate">{tag.name}</span>
        </Badge>
      {/if}
    {/each}
  </span>
{/if}
