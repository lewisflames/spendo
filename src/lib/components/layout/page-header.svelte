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

<div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-3">
  <div class="flex min-w-0 flex-1 flex-col gap-1">
    <h2 class="text-3xl font-bold tracking-tight break-words">{title}</h2>
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
    <div class="flex shrink-0 flex-wrap items-center gap-2">
      {@render children()}
    </div>
  {/if}
</div>
