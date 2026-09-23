<script lang="ts">
  import * as Dialog from '$lib/components/ui/dialog';
  import Kbd from '$lib/components/shared/kbd.svelte';
  import { NAV_GROUPS } from '$lib/features/nav/nav-items';
  import { keyboardShortcutsDialog } from '$lib/stores/ui.svelte';
  import { modKeyLabel } from '$lib/utils/platform';

  /** The layout accepts Cmd on macOS; show the chord users actually press. */
  const modKey = modKeyLabel();

  interface ShortcutEntry {
    keys: string[];
    description: string;
  }

  const globalShortcuts: ShortcutEntry[] = [
    { keys: [modKey, 'K'], description: 'Open command palette' },
    { keys: ['?'], description: 'Open keyboard shortcuts help' },
    { keys: ['Alt', '←'], description: 'Go back' },
    { keys: ['Alt', '→'], description: 'Go forward' },
    { keys: [modKey, 'Shift', 'T'], description: 'Toggle theme' },
    { keys: [modKey, 'B'], description: 'Toggle sidebar' }
  ];

  const navShortcuts: ShortcutEntry[] = NAV_GROUPS.flatMap((group) => group.items).flatMap(
    (item) =>
      item.chord
        ? [{ keys: ['G', item.chord.toUpperCase()], description: `Go to ${item.label}` }]
        : []
  );
</script>

<Dialog.Root bind:open={keyboardShortcutsDialog.value}>
  <Dialog.Content class="sm:max-w-sm">
    <Dialog.Header>
      <Dialog.Title>Keyboard Shortcuts</Dialog.Title>
      <Dialog.Description>Move a little faster with these.</Dialog.Description>
    </Dialog.Header>
    <div class="flex flex-col gap-3">
      {#snippet shortcutList(title: string, shortcuts: ShortcutEntry[])}
        <div class="flex flex-col gap-1.5">
          <h4 class="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
            {title}
          </h4>
          {#each shortcuts as shortcut (shortcut.keys.join('-'))}
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
      {/snippet}
      {@render shortcutList('Navigation', navShortcuts)}
      {@render shortcutList('Global', globalShortcuts)}
    </div>
  </Dialog.Content>
</Dialog.Root>
