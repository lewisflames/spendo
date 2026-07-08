<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { ArrowUp, ArrowDown, ArrowUpDown } from '@lucide/svelte';

  let {
    label,
    active = false,
    dir = 'desc',
    onclick,
    class: className = ''
  }: {
    label: string;
    active?: boolean;
    dir?: 'asc' | 'desc';
    onclick: () => void;
    class?: string;
  } = $props();
</script>

<Table.Head
  class="text-muted-foreground {className}"
  aria-sort={active ? (dir === 'asc' ? 'ascending' : 'descending') : 'none'}
>
  <button
    type="button"
    class="inline-flex items-center gap-1 transition-colors hover:text-foreground {active
      ? 'text-foreground'
      : ''}"
    {onclick}
  >
    {label}
    {#if active}
      {#if dir === 'asc'}
        <ArrowUp aria-hidden="true" class="size-3" />
      {:else}
        <ArrowDown aria-hidden="true" class="size-3" />
      {/if}
    {:else}
      <ArrowUpDown aria-hidden="true" class="size-3 opacity-50" />
    {/if}
  </button>
</Table.Head>
