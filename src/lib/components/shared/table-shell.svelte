<script lang="ts">
  import * as Table from '$lib/components/ui/table';
  import type { Snippet } from 'svelte';

  let {
    children,
    class: className = ''
  }: {
    children: Snippet;
    class?: string;
  } = $props();
</script>

<div
  class="overflow-auto rounded-lg border bg-muted/25 [&_tbody_td]:px-4 [&_thead_th]:px-4 [&_thead_th]:py-3 {className}"
>
  <Table.Root class="border-collapse">{@render children()}</Table.Root>
</div>
