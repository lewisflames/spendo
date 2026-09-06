<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { resolve } from '$app/paths';
  import { treeIndentStyle } from '$lib/utils';

  let {
    depth,
    guid,
    name,
    description
  }: {
    depth: number;
    /** Empty for synthetic rows (renders plain text, no account link). */
    guid: string;
    name: string;
    description?: string | null;
  } = $props();
</script>

<Table.Cell class="p-0">
  <div class="flex items-center gap-1 py-2.5" style={treeIndentStyle(depth)}>
    {#if guid}
      <a
        href={resolve('/accounts/[guid]', { guid })}
        title={description ?? name}
        class="block min-w-0 truncate text-sm font-medium focus-visible:outline-2 focus-visible:outline-ring"
      >
        {description ?? name}
      </a>
    {:else}
      <span title={description ?? name} class="min-w-0 truncate text-sm font-medium">
        {description ?? name}
      </span>
    {/if}
  </div>
</Table.Cell>
