<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ImportButton from '$lib/components/shared/import-button.svelte';
  import { FileUp } from '@lucide/svelte';
  import type { Snippet } from 'svelte';

  let { action: actionSnippet }: { action?: Snippet } = $props();
</script>

<Card.Root>
  <Card.Content class="p-0">
    <EmptyState
      title="Import a GnuCash file first"
      description="Load a .gnucash file to explore this data."
    >
      {#snippet icon()}
        <FileUp aria-hidden="true" class="text-muted-foreground" />
      {/snippet}
      {#snippet action()}
        {#if actionSnippet}
          {@render actionSnippet()}
        {:else}
          <ImportButton />
        {/if}
      {/snippet}
    </EmptyState>
  </Card.Content>
</Card.Root>
