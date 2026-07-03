<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Upload } from '@lucide/svelte';
  import { goto } from '$app/navigation';

  let {
    title = 'No Data',
    description = 'Upload a GnuCash file to get started.',
    icon
  }: {
    title?: string;
    description?: string;
    icon?: import('svelte').Snippet;
  } = $props();
</script>

<div class="flex flex-col items-center gap-4 py-12 text-center">
  {#if icon}
    {@render icon()}
  {:else}
    <div aria-hidden="true" class="flex size-16 items-center justify-center rounded-full bg-muted">
      <Upload aria-hidden="true" class="size-8 text-muted-foreground" />
    </div>
  {/if}
  <div class="flex flex-col gap-1">
    <h2 class="text-2xl font-bold tracking-tight">{title}</h2>
    <p class="text-sm text-muted-foreground">{description}</p>
  </div>
  <Button onclick={() => goto('/')}>
    <Upload aria-hidden="true" class="mr-1.5 size-3.5" />
    Upload .gnucash file
  </Button>
</div>
