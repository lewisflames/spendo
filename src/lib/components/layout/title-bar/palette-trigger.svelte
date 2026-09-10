<script lang="ts">
  import { Search } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { commandPalette } from '$lib/stores/ui.svelte';
  import { modKeyLabel } from '$lib/utils/platform';

  /** The layout accepts Cmd on macOS; show the chord users actually press. */
  const modKey = modKeyLabel();
</script>

<Button
  variant="ghost"
  size="icon-sm"
  class="text-muted-foreground"
  aria-label="Open command palette"
  title={`Command palette (${modKey}+K)`}
  onclick={() => (commandPalette.value = true)}
>
  <Search class="size-4" />
</Button>
