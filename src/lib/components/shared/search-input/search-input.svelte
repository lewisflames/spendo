<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  import { Search } from '@lucide/svelte';

  let {
    id,
    label,
    placeholder = 'Search...',
    value = $bindable(''),
    class: className = ''
  }: {
    id: string;
    /** Accessible name; visually hidden. Defaults to the placeholder. */
    label?: string;
    placeholder?: string;
    value?: string;
    /** Extra classes for the outer wrapper (e.g. grow basis-48). */
    class?: string;
  } = $props();
</script>

<div class="relative {className}">
  <label for={id} class="sr-only">{label ?? placeholder}</label>
  <Search
    aria-hidden="true"
    class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
  />
  <Input {id} {placeholder} bind:value class="pl-8" />
</div>
