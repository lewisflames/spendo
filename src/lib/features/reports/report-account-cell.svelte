<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { resolve } from '$app/paths';
  import { treeIndentStyle } from '$lib/utils';
  import AccountLabel from '$lib/features/accounts/account-label.svelte';

  let {
    depth,
    guid,
    name,
    description,
    code
  }: {
    depth: number;
    /** Empty for synthetic rows (renders plain text, no account link). */
    guid: string;
    name: string;
    description?: string | null;
    /** Optional account code, shown as a small chip after the name. */
    code?: string | null;
  } = $props();
</script>

<Table.Cell class="p-0">
  <div class="flex items-center gap-1 py-2.5" style={treeIndentStyle(depth)}>
    {#if guid}
      <a
        href={resolve('/accounts/[guid]', { guid })}
        class="block min-w-0 text-sm focus-visible:outline-2 focus-visible:outline-ring"
      >
        <AccountLabel {name} {description} {code} />
      </a>
    {:else}
      <span class="min-w-0 text-sm">
        <AccountLabel {name} {description} {code} />
      </span>
    {/if}
  </div>
</Table.Cell>
