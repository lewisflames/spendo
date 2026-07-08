<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title,
    description,
    children,
    class: className = ''
  }: {
    title: string;
    description?: string | Snippet;
    children?: Snippet;
    class?: string;
  } = $props();
</script>

<div class="flex items-center justify-between gap-4 {className}">
  <div class="flex flex-1 flex-col gap-1">
    <h2 class="text-3xl font-bold tracking-tight">{title}</h2>
    {#if description}
      {#if typeof description === 'function'}
        <p class="text-sm text-muted-foreground">{@render description()}</p>
      {:else}
        <p class="text-sm text-muted-foreground">{description}</p>
      {/if}
    {/if}
  </div>
  {#if children}
    {@render children()}
  {/if}
</div>
