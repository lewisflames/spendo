<script lang="ts">
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { Button } from '$lib/components/ui/button';
  import { resolve } from '$app/paths';
  import { FileUp, Settings } from '@lucide/svelte';
</script>

<EmptyState
  card
  title="No data in this book yet"
  description="Import a GnuCash file or bank CSV from Settings → Data import to start exploring."
>
  {#snippet icon()}
    <FileUp aria-hidden="true" class="text-muted-foreground" />
  {/snippet}
  {#snippet action()}
    <Button variant="outline" href={resolve('/settings')}>
      <Settings aria-hidden="true" class="size-4" />
      Settings
    </Button>
  {/snippet}
</EmptyState>
