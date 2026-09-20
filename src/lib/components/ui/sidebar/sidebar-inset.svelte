<script lang="ts">
  import { cn, type WithElementRef } from '$lib/utils.js';
  import type { HTMLAttributes } from 'svelte/elements';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLElement>> = $props();
</script>

<main
  bind:this={ref}
  data-slot="sidebar-inset"
  class={cn(
    'relative flex w-full flex-1 flex-col bg-background peer-data-[variant=inset]:m-2 peer-data-[variant=inset]:mt-0 peer-data-[variant=inset]:rounded-xl peer-data-[variant=inset]:shadow-sm',
    className
  )}
  {...restProps}
>
  {@render children?.()}
</main>
