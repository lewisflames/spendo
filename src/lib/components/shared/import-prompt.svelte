<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import ImportButton from '$lib/components/shared/import-button.svelte';
  import { FileUp } from '@lucide/svelte';
</script>

<Card.Root>
  <Card.Content class="p-0">
    <EmptyState
      title="No data in this book yet"
      description="Import from GnuCash to start exploring."
    >
      {#snippet icon()}
        <FileUp aria-hidden="true" class="text-muted-foreground" />
      {/snippet}
      {#snippet action()}
        <ImportButton />
      {/snippet}
    </EmptyState>
  </Card.Content>
</Card.Root>
