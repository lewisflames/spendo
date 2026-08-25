<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import Kbd from '$lib/components/shared/kbd.svelte';
  import { keyboardShortcutsDialog } from '$lib/stores/keyboard-shortcuts.svelte';

  /** The layout accepts Cmd on macOS; show the chord users actually press. */
  const modKey = navigator.platform.toLowerCase().includes('mac') ? '⌘' : 'Ctrl';

  interface ShortcutEntry {
    keys: string[];
    description: string;
  }

  const globalShortcuts: ShortcutEntry[] = [
    { keys: ['?'], description: 'Open keyboard shortcuts help' },
    { keys: ['Alt', '←'], description: 'Go back' },
    { keys: ['Alt', '→'], description: 'Go forward' },
    { keys: [modKey, 'Shift', 'T'], description: 'Toggle theme' },
    { keys: [modKey, 'B'], description: 'Toggle sidebar' }
  ];
</script>

<Dialog.Root bind:open={keyboardShortcutsDialog.value}>
  <Dialog.Content class="sm:max-w-sm">
    <Dialog.Header>
      <Dialog.Title>Keyboard Shortcuts</Dialog.Title>
      <Dialog.Description>Available keyboard shortcuts for Spendo.</Dialog.Description>
    </Dialog.Header>
    <div class="flex flex-col gap-3">
      <div class="flex flex-col gap-1.5">
        <h4 class="text-xs font-semibold tracking-wider text-muted-foreground uppercase">Global</h4>
        {#each globalShortcuts as shortcut (shortcut.keys.join('-'))}
          <div class="flex items-center justify-between gap-4 py-0.5">
            <span class="text-sm text-foreground">{shortcut.description}</span>
            <div class="flex items-center gap-0.5">
              {#each shortcut.keys as key, i (key)}
                <Kbd>{key}</Kbd>
                {#if i < shortcut.keys.length - 1}
                  <span class="text-xs text-muted-foreground">+</span>
                {/if}
              {/each}
            </div>
          </div>
        {/each}
      </div>
    </div>
  </Dialog.Content>
</Dialog.Root>
