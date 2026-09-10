<script lang="ts">
  import * as Card from '$lib/components/ui/card';
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { FileUp } from '@lucide/svelte';
</script>

<Card.Root>
  <Card.Content class="p-0">
    <EmptyState
      title="No data in this book yet"
      description="Import a GnuCash file or bank CSV from Settings → Data import to start exploring."
    >
      {#snippet icon()}
        <FileUp aria-hidden="true" class="text-muted-foreground" />
      {/snippet}
    </EmptyState>
  </Card.Content>
</Card.Root>
