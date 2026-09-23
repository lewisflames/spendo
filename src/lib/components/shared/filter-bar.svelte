<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    children,
    extra
  }: {
    children: Snippet;
    /** Optional trailing controls (e.g. a columns toggle), right-aligned. */
    extra?: Snippet;
  } = $props();
</script>

<div class="flex flex-wrap items-end gap-2">
  {@render children()}
  {#if extra}
    <div class="ml-auto flex items-center gap-2">{@render extra()}</div>
  {/if}
</div>
