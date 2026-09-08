<script lang="ts">
  import * as Command from '$lib/components/ui/command';
  import * as Dialog from '$lib/components/ui/dialog';
  import Kbd from '$lib/components/shared/kbd.svelte';
  import {
    PALETTE_COMMANDS,
    PALETTE_GROUPS,
    type PaletteCommand
  } from '$lib/features/palette/commands';
  import { commandPalette } from '$lib/stores/ui.svelte';
  import { dbSession } from '$lib/stores/db.svelte';

  let search = $state('');

  // Fresh query on every open so a previous search never leaks through.
  $effect(() => {
    if (!commandPalette.value) search = '';
  });

  // "Go to" targets redirect home while book-less (layout guard), so hide
  // them in the welcome shell instead of offering dead ends.
  let groups = $derived(
    PALETTE_GROUPS.filter((group) => group !== 'Go to' || dbSession.value !== null).map(
      (group) => ({
        name: group,
        items: PALETTE_COMMANDS.filter((cmd) => cmd.group === group)
      })
    )
  );

  function select(cmd: PaletteCommand) {
    commandPalette.value = false;
    void cmd.run();
  }
</script>

<Dialog.Root bind:open={commandPalette.value}>
  <Dialog.Content
    class="top-[12vh] translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-lg"
    showCloseButton={false}
  >
    <Dialog.Header class="sr-only">
      <Dialog.Title>Command palette</Dialog.Title>
      <Dialog.Description>Jump to a page or run an action.</Dialog.Description>
    </Dialog.Header>
    <Command.Root label="Command palette" loop>
      <Command.Input bind:value={search} placeholder="Search or jump to…" autofocus />
      <Command.List>
        <Command.Empty>No matching commands.</Command.Empty>
        {#each groups as group (group.name)}
          <Command.Group heading={group.name}>
            {#each group.items as cmd (cmd.id)}
              <Command.Item value={cmd.title} keywords={cmd.keywords} onSelect={() => select(cmd)}>
                <cmd.icon aria-hidden="true" />
                <span class="min-w-0 flex-1 truncate">{cmd.title}</span>
              </Command.Item>
            {/each}
          </Command.Group>
        {/each}
      </Command.List>
      <div
        class="flex items-center gap-3 border-t px-3 py-2 text-xs text-muted-foreground"
        aria-hidden="true"
      >
        <span class="flex items-center gap-1"><Kbd>↑</Kbd><Kbd>↓</Kbd> navigate</span>
        <span class="flex items-center gap-1"><Kbd>↵</Kbd> select</span>
        <span class="flex items-center gap-1"><Kbd>esc</Kbd> close</span>
      </div>
    </Command.Root>
  </Dialog.Content>
</Dialog.Root>
