<script lang="ts">
  import { Command as CommandPrimitive } from 'bits-ui';
  import SearchIcon from '@lucide/svelte/icons/search';
  import { cn } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    value = $bindable(''),
    class: className,
    ...restProps
  }: CommandPrimitive.InputProps = $props();
</script>

<div
  data-slot="command-input-wrapper"
  class="flex h-10 items-center gap-2 border-b px-3 [&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:opacity-50"
>
  <SearchIcon aria-hidden="true" />
  <CommandPrimitive.Input
    bind:ref
    bind:value
    data-slot="command-input"
    class={cn(
      'w-full bg-transparent text-sm outline-hidden placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-50',
      className
    )}
    {...restProps}
  />
</div>
