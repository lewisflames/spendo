<script lang="ts">
  import { browser } from '$app/environment';
  import SearchIcon from '@lucide/svelte/icons/search';
  import { Button } from '$lib/components/ui/button';
  import { commandPalette } from '$lib/stores/ui.svelte';

  /** The layout accepts Cmd on macOS; show the chord users actually press. */
  const modKey = browser && navigator.platform.toLowerCase().includes('mac') ? '⌘' : 'Ctrl';
</script>

<Button
  variant="ghost"
  size="icon-sm"
  class="text-muted-foreground"
  aria-label="Open command palette"
  title={`Command palette (${modKey}+K)`}
  onclick={() => (commandPalette.value = true)}
>
  <SearchIcon class="size-4" />
</Button>
