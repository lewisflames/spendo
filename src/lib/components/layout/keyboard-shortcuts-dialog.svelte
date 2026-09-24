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
    /** Keys are pressed one after another (g then t), not held together. */
    sequence?: boolean;
  }

  const globalShortcuts: ShortcutEntry[] = [
    { keys: [modKey, 'K'], description: 'Open command palette' },
    { keys: ['?'], description: 'Open keyboard shortcuts help' },
    { keys: ['Alt', '←'], description: 'Go back' },
    { keys: ['Alt', '→'], description: 'Go forward' },
    { keys: [modKey, 'Shift', 'T'], description: 'Toggle theme' },
    { keys: [modKey, 'B'], description: 'Toggle sidebar' }
  ];

  const formShortcuts: ShortcutEntry[] = [
    { keys: [modKey, 'S'], description: 'Save the open form' },
    { keys: [modKey, 'Shift', 'S'], description: 'Save & add another (new transactions)' }
  ];

  const navShortcuts: ShortcutEntry[] = NAV_GROUPS.flatMap((group) => group.items).flatMap(
    (item) =>
      item.chord
        ? [
            {
              keys: ['G', item.chord.toUpperCase()],
              description: `Go to ${item.label}`,
              sequence: true
            }
          ]
        : []
  );
</script>

<Dialog.Root bind:open={keyboardShortcutsDialog.value}>
  <Dialog.Content class="sm:max-w-sm">
    <Dialog.Header>
      <Dialog.Title>Keyboard shortcuts</Dialog.Title>
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
                    <span class="px-0.5 text-xs text-muted-foreground"
                      >{shortcut.sequence ? 'then' : '+'}</span
                    >
                  {/if}
                {/each}
              </div>
            </div>
          {/each}
        </div>
      {/snippet}
      {@render shortcutList('Navigation', navShortcuts)}
      {@render shortcutList('Global', globalShortcuts)}
      {@render shortcutList('Forms', formShortcuts)}
    </div>
  </Dialog.Content>
</Dialog.Root>
