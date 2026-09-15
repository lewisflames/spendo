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
  import { useDebouncedSearch } from '$lib/hooks/use-debounced-search.svelte';
  import { useAsyncData } from '$lib/hooks/use-async-data.svelte';
  import { searchGlobal } from '$lib/tauri';
  import { openAccount, openBudget, openHolding, openTransaction } from '$lib/utils/navigation';
  import { formatDate } from '$lib/utils/format.svelte';
  import { ArrowLeftRight, Briefcase, FolderTree, PiggyBank } from '@lucide/svelte';

  let search = $state('');

  // Fresh query on every open so a previous search never leaks through.
  $effect(() => {
    if (!commandPalette.value) search = '';
  });

  // Backend filtering is server-side (LIKE), so command filtering is manual
  // with `shouldFilter={false}` below. Debounce at 200ms — snappier than the
  // 300ms page default since the palette feels like instant jump.
  let debounced = useDebouncedSearch(
    () => search,
    (v) => v.trim(),
    200
  );
  let query = $derived(debounced.value);
  let queryActive = $derived(
    commandPalette.value && dbSession.value !== null && [...query].length >= 2
  );

  const results = useAsyncData({
    enabled: () => queryActive,
    fetch: () => searchGlobal(query, 5),
    label: 'Failed to search'
  });

  function matchesQuery(cmd: PaletteCommand, q: string): boolean {
    const needle = q.trim().toLowerCase();
    if (!needle) return true;
    if (cmd.title.toLowerCase().includes(needle)) return true;
    return cmd.keywords.some((k) => k.toLowerCase().includes(needle));
  }

  // "Go to" targets redirect home while book-less (layout guard), so hide
  // them in the welcome shell instead of offering dead ends.
  let groups = $derived(
    PALETTE_GROUPS.filter((group) => group !== 'Go to' || dbSession.value !== null).map(
      (group) => ({
        name: group,
        items: PALETTE_COMMANDS.filter(
          (cmd) => cmd.group === group && matchesQuery(cmd, search) && cmd.when?.() !== false
        )
      })
    )
  );

  let hasEntityHits = $derived(
    (results.data?.accounts.length ?? 0) > 0 ||
      (results.data?.transactions.length ?? 0) > 0 ||
      (results.data?.holdings.length ?? 0) > 0 ||
      (results.data?.budgets.length ?? 0) > 0
  );

  function select(cmd: PaletteCommand) {
    commandPalette.value = false;
    void cmd.run();
  }

  function selectAccount(guid: string) {
    commandPalette.value = false;
    void openAccount(guid);
  }

  function selectTransaction(guid: string) {
    commandPalette.value = false;
    void openTransaction(guid);
  }

  function selectHolding(commodityGuid: string) {
    commandPalette.value = false;
    void openHolding(commodityGuid);
  }

  function selectBudget(accountGuid: string, year: number) {
    commandPalette.value = false;
    void openBudget(accountGuid, year);
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
    <Command.Root label="Command palette" loop shouldFilter={false}>
      <Command.Input bind:value={search} placeholder="Search or jump to…" autofocus />
      <Command.List>
        <Command.Empty>No matching commands or results.</Command.Empty>
        {#each groups as group (group.name)}
          {#if group.items.length > 0}
            <Command.Group heading={group.name}>
              {#each group.items as cmd (cmd.id)}
                <Command.Item value={cmd.id} keywords={cmd.keywords} onSelect={() => select(cmd)}>
                  <cmd.icon aria-hidden="true" />
                  <span class="min-w-0 flex-1 truncate">{cmd.title}</span>
                </Command.Item>
              {/each}
            </Command.Group>
          {/if}
        {/each}
        {#if queryActive}
          {#if results.loading && !results.data}
            <Command.Group heading="Searching…">
              <Command.Item value="searching" disabled>
                <span class="min-w-0 flex-1 truncate text-muted-foreground">Searching…</span>
              </Command.Item>
            </Command.Group>
          {:else if results.error}
            <Command.Group heading="Search">
              <Command.Item value="search-error" disabled>
                <span class="min-w-0 flex-1 truncate text-destructive">{results.error}</span>
              </Command.Item>
            </Command.Group>
          {:else if results.data && hasEntityHits}
            {#if results.data.accounts.length > 0}
              <Command.Group heading="Accounts">
                {#each results.data.accounts as a (a.guid)}
                  <Command.Item
                    value={`account-${a.guid}`}
                    keywords={[a.name, a.fullPath]}
                    onSelect={() => selectAccount(a.guid)}
                  >
                    <FolderTree aria-hidden="true" />
                    <span class="flex min-w-0 flex-1 flex-col">
                      <span class="truncate">{a.name}</span>
                      <span class="truncate text-xs text-muted-foreground">{a.fullPath}</span>
                    </span>
                  </Command.Item>
                {/each}
              </Command.Group>
            {/if}
            {#if results.data.transactions.length > 0}
              <Command.Group heading="Transactions">
                {#each results.data.transactions as t (t.guid)}
                  <Command.Item
                    value={`transaction-${t.guid}`}
                    keywords={[t.description ?? '', t.num ?? '']}
                    onSelect={() => selectTransaction(t.guid)}
                  >
                    <ArrowLeftRight aria-hidden="true" />
                    <span class="flex min-w-0 flex-1 flex-col">
                      <span class="truncate">{t.description ?? t.num ?? 'Transaction'}</span>
                      <span class="truncate text-xs text-muted-foreground">
                        {formatDate(t.postDate)}{t.num ? ` · ${t.num}` : ''}
                      </span>
                    </span>
                  </Command.Item>
                {/each}
              </Command.Group>
            {/if}
            {#if results.data.holdings.length > 0}
              <Command.Group heading="Holdings">
                {#each results.data.holdings as h (h.commodityGuid)}
                  <Command.Item
                    value={`holding-${h.commodityGuid}`}
                    keywords={[h.mnemonic, h.fullName ?? '']}
                    onSelect={() => selectHolding(h.commodityGuid)}
                  >
                    <Briefcase aria-hidden="true" />
                    <span class="flex min-w-0 flex-1 flex-col">
                      <span class="truncate">{h.fullName ?? h.mnemonic}</span>
                      <span class="truncate text-xs text-muted-foreground">{h.mnemonic}</span>
                    </span>
                  </Command.Item>
                {/each}
              </Command.Group>
            {/if}
            {#if results.data.budgets.length > 0}
              <Command.Group heading="Budgets">
                {#each results.data.budgets as b (b.accountGuid)}
                  <Command.Item
                    value={`budget-${b.accountGuid}`}
                    keywords={[b.accountName]}
                    onSelect={() => selectBudget(b.accountGuid, b.year)}
                  >
                    <PiggyBank aria-hidden="true" />
                    <span class="flex min-w-0 flex-1 flex-col">
                      <span class="truncate">{b.accountName}</span>
                      <span class="truncate text-xs text-muted-foreground">{b.year}</span>
                    </span>
                  </Command.Item>
                {/each}
              </Command.Group>
            {/if}
          {/if}
        {/if}
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
