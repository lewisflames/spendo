<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    active = false,
    children,
    class: className = ''
  }: {
    /** When true, dims the content and blocks pointer input during refetches. */
    active?: boolean;
    children: Snippet;
    class?: string;
  } = $props();
</script>

<div
  class="transition-opacity {className}"
  class:opacity-50={active}
  class:pointer-events-none={active}
>
  {@render children()}
</div>
