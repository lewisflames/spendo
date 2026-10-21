<script lang="ts">
  import EmptyState from '$lib/components/shared/empty-state.svelte';
  import { Button } from '$lib/components/ui/button';
  import { resolve } from '$app/paths';
  import { FileUp, Settings } from '@lucide/svelte';
  import { modKeyLabel } from '$lib/utils/platform';

  const modKey = modKeyLabel();
</script>

<EmptyState
  card
  title="This book is empty"
  description={`Bring in a GnuCash file or a bank CSV from the book menu at the bottom of the sidebar, or press ${modKey}+K and search for “import”. Imported accounts will show up here.`}
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
