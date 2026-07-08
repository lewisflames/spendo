<script lang="ts">
  import { cn, type WithElementRef } from '$lib/utils.js';
  import type { HTMLAttributes } from 'svelte/elements';
  import { useSidebar } from './context.svelte.js';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();

  const sidebar = useSidebar();
</script>

<div
  bind:this={ref}
  class="group peer block text-sidebar-foreground"
  data-state={sidebar.collapsed ? 'collapsed' : 'expanded'}
  data-collapsible={sidebar.collapsed ? 'icon' : 'none'}
  data-variant="inset"
  data-side="left"
  data-slot="sidebar"
>
  <div
    data-slot="sidebar-gap"
    class="relative w-(--sidebar-width) bg-transparent transition-[width] duration-200 ease-linear group-data-[collapsible=icon]:w-[calc(var(--sidebar-width-icon)+(--spacing(4)))]"
  ></div>
  <div
    data-slot="sidebar-container"
    class={cn(
      'fixed inset-s-0 z-10 flex w-(--sidebar-width) transition-[left,right,width] duration-200 ease-linear group-data-[collapsible=icon]:w-[calc(var(--sidebar-width-icon)+(--spacing(4))+2px)]',
      className
    )}
    {...restProps}
  >
    <div
      data-sidebar="sidebar"
      data-slot="sidebar-inner"
      class="flex size-full flex-col bg-sidebar"
    >
      {@render children?.()}
    </div>
  </div>
</div>
