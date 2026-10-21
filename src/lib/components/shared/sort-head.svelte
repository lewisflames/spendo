<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import { ArrowUp, ArrowDown, ArrowUpDown } from '@lucide/svelte';

  let {
    label,
    active = false,
    dir = 'desc',
    onclick,
    class: className = '',
    align = 'left'
  }: {
    label: string;
    active?: boolean;
    dir?: 'asc' | 'desc';
    /** Omit for a static header (e.g. a preview table that can’t re-sort). */
    onclick?: () => void;
    class?: string;
    /** Horizontal alignment; `right` right-aligns the sort button for numeric columns. */
    align?: 'left' | 'right';
  } = $props();

  let alignClass = $derived(align === 'right' ? 'w-full justify-end text-right' : '');
</script>

<Table.Head
  class="text-muted-foreground {className}"
  aria-sort={onclick ? (active ? (dir === 'asc' ? 'ascending' : 'descending') : 'none') : undefined}
>
  {#if !onclick}
    <span class="inline-flex {alignClass}">{label}</span>
  {:else}
    <button
      type="button"
      class="inline-flex items-center gap-1 transition-colors hover:text-foreground {alignClass} {active
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
  {/if}
</Table.Head>
