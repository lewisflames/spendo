<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title,
    description,
    children
  }: {
    title: string;
    description?: string | Snippet;
    children?: Snippet;
  } = $props();
</script>

<div class="flex items-center justify-between gap-4">
  <div class="flex flex-1 flex-col gap-1">
    <h2 class="text-3xl font-bold tracking-tight">{title}</h2>
    {#if description !== undefined}
      {#if typeof description === 'function'}
        <!-- min-h-5 reserves the text-sm line height so an empty description
             (e.g. before data loads) doesn't collapse and shift the page. -->
        <p class="min-h-5 text-sm text-muted-foreground">{@render description()}</p>
      {:else}
        <p class="min-h-5 text-sm text-muted-foreground">{description}</p>
      {/if}
    {/if}
  </div>
  {#if children}
    {@render children()}
  {/if}
</div>
