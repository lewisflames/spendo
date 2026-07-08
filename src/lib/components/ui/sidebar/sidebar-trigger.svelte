<script lang="ts">
  import { PanelLeftClose, PanelLeftOpen } from '@lucide/svelte';
  import { cn } from '$lib/utils.js';
  import { Button } from '$lib/components/ui/button';
  import { useSidebar } from './context.svelte.js';

  let {
    class: className,
    title
  }: {
    class?: string;
    title?: string;
  } = $props();

  const sidebar = useSidebar();

  const label = $derived(sidebar.collapsed ? 'Expand sidebar' : 'Collapse sidebar');
</script>

<Button
  data-slot="sidebar-trigger"
  data-sidebar="trigger"
  variant="ghost"
  size="icon-sm"
  class={cn('shrink-0', className)}
  onclick={() => sidebar.toggle()}
  aria-label={label}
  title={title ?? label}
>
  {#if sidebar.collapsed}
    <PanelLeftOpen class="size-4" />
  {:else}
    <PanelLeftClose class="size-4" />
  {/if}
</Button>
