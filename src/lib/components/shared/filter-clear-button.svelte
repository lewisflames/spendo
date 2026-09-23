<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { X } from '@lucide/svelte';

  let {
    label = 'Clear filters',
    onclick
  }: {
    /** Accessible label; defaults to "Clear filters". */
    label?: string;
    onclick: () => void;
  } = $props();
</script>

<Button variant="ghost" {onclick}>
  <X aria-hidden="true" class="mr-1 size-3.5" />
  {label}
</Button>
