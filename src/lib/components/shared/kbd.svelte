<script lang="ts">
  import { cn, type WithElementRef } from '$lib/utils.js';
  import type { HTMLAttributes } from 'svelte/elements';
  import type { Snippet } from 'svelte';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLElement>> & {
    children?: Snippet;
  } = $props();
</script>

<kbd
  bind:this={ref}
  data-slot="kbd"
  class={cn(
    'inline-flex items-center justify-center rounded-md border bg-muted px-1.5 py-0.5 font-mono text-[10px] font-medium text-muted-foreground',
    className
  )}
  {...restProps}
>
  {@render children?.()}
</kbd>
