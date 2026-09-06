<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { resolve } from '$app/paths';
  import TreeChevron from '$lib/components/shared/tree-chevron.svelte';
  import { treeIndentStyle } from '$lib/utils';

  let {
    depth,
    guid,
    name,
    description,
    hasChildren,
    expanded,
    onToggle
  }: {
    depth: number;
    /** Empty for synthetic rows (renders plain text, no account link). */
    guid: string;
    name: string;
    description?: string | null;
    hasChildren: boolean;
    expanded: boolean;
    onToggle: () => void;
  } = $props();
</script>

<Table.Cell class="p-0">
  <div class="flex items-center gap-1 py-1.5" style={treeIndentStyle(depth)}>
    <TreeChevron open={expanded} interactive={hasChildren} ontoggle={onToggle} />
    {#if guid}
      <a
        href={resolve('/accounts/[guid]', { guid })}
        class="min-w-0 truncate text-sm font-medium hover:underline"
      >
        {description ?? name}
      </a>
    {:else}
      <span class="min-w-0 truncate text-sm font-medium">
        {description ?? name}
      </span>
    {/if}
  </div>
</Table.Cell>
