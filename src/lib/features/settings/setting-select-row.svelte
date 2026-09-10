<script lang="ts">
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils';

  let {
    label,
    description,
    class: className = '',
    children
  }: {
    label: string;
    description: string;
    class?: string;
    children: Snippet;
  } = $props();
</script>

<div class={cn('flex items-center justify-between gap-4', className)}>
  <div class="flex flex-col gap-1">
    <span class="text-sm font-medium">{label}</span>
    <span class="text-xs text-muted-foreground">{description}</span>
  </div>
  {@render children()}
</div>
