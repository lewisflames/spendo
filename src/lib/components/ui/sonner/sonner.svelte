<script lang="ts">
  import { Toaster } from 'svelte-sonner';
  import type { ComponentProps } from 'svelte';

  let {
    richColors = true,
    position = 'bottom-right',
    closeButton = false,
    ...rest
  }: ComponentProps<typeof Toaster> = $props();
</script>

<Toaster {richColors} {position} {closeButton} {...rest} />
