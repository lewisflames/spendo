<script lang="ts">
  import * as Command from '$lib/components/ui/command';
  import Kbd from '$lib/components/shared/kbd.svelte';
  import AccountLabel from '$lib/features/accounts/account-label.svelte';
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
  import { modKeyLabel } from '$lib/utils/platform';
  import { ArrowLeftRight, Briefcase, FolderTree, PiggyBank, Search } from '@lucide/svelte';
  import { Command as CommandPrimitive } from 'bits-ui';

  /** The layout accepts Cmd on macOS; show the chord users actually press. */
  const modKey = modKeyLabel();

  // The title-bar bar is the palette’s only input; `search` drives filtering
  // for both the command groups and the backend entity search below.
  let search = $state('');
  let barEl = $state<HTMLElement | null>(null);
  let panelEl = $state<HTMLElement | null>(null);
  let inputEl = $state<HTMLElement | null>(null);

  // The results panel drops down from the bar: same x/width, top flush with
  // the header. Re-measured on every open and kept in sync while resizing.
  let panelRect = $state<{ left: number; top: number; width: number } | null>(null);

  $effect(() => {
    if (!commandPalette.value) return;
    const el = barEl;
    if (!el) return;
    const measure = () => {
      const rect = el.getBoundingClientRect();
      panelRect = { left: rect.left, top: rect.bottom + 6, width: rect.width };
    };
    measure();
    window.addEventListener('resize', measure);
    return () => window.removeEventListener('resize', measure);
  });

  // Hidden until measured so the panel never flashes at the viewport origin.
  const panelStyle = $derived(
    panelRect
      ? `left: ${panelRect.left}px; top: ${panelRect.top}px; width: ${panelRect.width}px;`
      : 'visibility: hidden;'
  );

  // Opening from anywhere (⌘K, palette commands) focuses the bar so typing
  // lands in the title bar.
  $effect(() => {
    if (!commandPalette.value) return;
    inputEl?.focus();
  });

  // Dismiss on Escape or any pointer interaction outside the bar/panel.
  $effect(() => {
    if (!commandPalette.value) return;
    const onPointerDown = (e: PointerEvent) => {
      if (!(e.target instanceof Node)) return;
      if (barEl?.contains(e.target) || panelEl?.contains(e.target)) return;
      commandPalette.value = false;
    };
    const onKeydown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') commandPalette.value = false;
    };
    window.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('keydown', onKeydown);
    return () => {
      window.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('keydown', onKeydown);
    };
  });

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
    label: 'We couldn’t search right now'
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

<!-- `contents` keeps the command root out of the header’s flex layout; the
     bar is the only in-flow child, the panel is fixed below it. -->
<Command.Root
  class="contents bg-transparent text-inherit"
  label="Command palette"
  loop
  shouldFilter={false}
>
  <!-- `select-none` is set on the header; re-enable selection in the input. -->
  <div
    bind:this={barEl}
    class="relative h-7 w-full max-w-md"
    onfocusout={(e) => {
      const next = e.relatedTarget;
      if (next instanceof Node && (barEl?.contains(next) || panelEl?.contains(next))) return;
      commandPalette.value = false;
    }}
  >
    <Search
      aria-hidden="true"
      class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
    />
    <CommandPrimitive.Input
      bind:ref={inputEl}
      bind:value={search}
      placeholder="Search or jump to…"
      aria-label="Search or jump to"
      class="h-full w-full min-w-0 rounded-lg border border-input bg-transparent pr-14 pl-8 text-sm text-foreground transition-colors outline-none select-text placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30"
      onfocus={() => (commandPalette.value = true)}
    />
    {#if !search && !commandPalette.value}
      <span
        aria-hidden="true"
        class="pointer-events-none absolute top-1/2 right-2 flex -translate-y-1/2 items-center gap-0.5"
      >
        <Kbd>{modKey}</Kbd>
        <Kbd>K</Kbd>
      </span>
    {/if}
  </div>

  {#if commandPalette.value}
    <!-- Prevent mousedown default so clicking a result keeps focus in the bar. -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      bind:this={panelEl}
      data-drag-ignore
      class="fixed z-50 grid animate-in gap-0 overflow-hidden rounded-lg bg-popover p-0 text-sm text-popover-foreground shadow-md ring-1 ring-foreground/10 duration-100 fade-in-0 slide-in-from-top-2"
      style={panelStyle}
      onmousedown={(e) => e.preventDefault()}
    >
      <Command.List>
        <Command.Empty>Nothing matches. Try something else.</Command.Empty>
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
                    keywords={[a.name, a.fullPath, a.description ?? '']}
                    title={a.description ? `${a.fullPath} · ${a.description}` : a.fullPath}
                    onSelect={() => selectAccount(a.guid)}
                  >
                    <FolderTree aria-hidden="true" />
                    <span class="min-w-0 flex-1">
                      <AccountLabel
                        name={a.name}
                        code={a.code}
                        description={a.description
                          ? `${a.fullPath} · ${a.description}`
                          : a.fullPath}
                      />
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
    </div>
  {/if}
</Command.Root>
