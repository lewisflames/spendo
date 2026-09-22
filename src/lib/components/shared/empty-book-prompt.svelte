<script lang="ts">
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { FileUp } from '@lucide/svelte';
</script>

<EmptyState
  card
  title="No data in this book yet"
  description="Import a GnuCash file or bank CSV from Settings → Data import to start exploring."
>
  {#snippet icon()}
    <FileUp aria-hidden="true" class="text-muted-foreground" />
  {/snippet}
</EmptyState>
