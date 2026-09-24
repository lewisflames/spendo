<script lang="ts">
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils';

  let {
    label,
    description,
    labelFor,
    class: className = '',
    children
  }: {
    label: string;
    description: string;
    /** When set, the label is a real `<label for>` pointing at the control. */
    labelFor?: string;
    class?: string;
    children: Snippet;
  } = $props();
</script>

<div class={cn('flex items-center justify-between gap-4', className)}>
  <div class="flex flex-col gap-1">
    {#if labelFor}
      <label for={labelFor} class="text-sm font-medium">{label}</label>
    {:else}
      <span class="text-sm font-medium">{label}</span>
    {/if}
    <span class="text-xs text-muted-foreground">{description}</span>
  </div>
  {@render children()}
</div>
