<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { untrack } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import { AppSidebar } from '$lib/components/layout/app-sidebar';
  import { HistorySidebar } from '$lib/components/layout/history-sidebar';
  import {
    refreshChangeHistory,
    redoLatestChange,
    undoLatestChange
  } from '$lib/features/history/history.svelte';
  import {
    appSettings,
    saveSettings,
    applyAccent,
    applyDensity,
    toggleTheme
  } from '$lib/stores/settings.svelte';
  import { goBack, goForward, initNavHistory } from '$lib/stores/nav-history.svelte';
  import {
    dbSession,
    dbBootstrapping,
    ensureDbSessionLoaded,
    loadActiveBook
  } from '$lib/stores/db.svelte';
  import { TitleBar } from '$lib/components/layout/title-bar';
  import { cn } from '$lib/utils';
  import { Sonner } from '$lib/components/ui/sonner';
  import KeyboardShortcutsDialog from '$lib/components/layout/keyboard-shortcuts-dialog.svelte';
  import CommandPalette from '$lib/components/layout/command-palette.svelte';
  import {
    keyboardShortcutsDialog as shortcutsDialog,
    commandPalette,
    historySidebar,
    toggleHistorySidebar
  } from '$lib/stores/ui.svelte';
  import '../app.css';

  let { children } = $props();

  initNavHistory();

  const prefersDark = new MediaQuery('(prefers-color-scheme: dark)');

  // Bootstrap DB state once so deep-linked/refreshed pages see real data
  // (e.g. Settings' "Clear all data" gate) instead of a stale null session.
  // The rejection is deliberately swallowed here: the store resets its
  // bootstrap flag, so a later navigation retries; pages that need the
  // session surface their own errors.
  $effect(() => {
    ensureDbSessionLoaded().catch(() => {});
    loadActiveBook().catch(() => {});
  });

  // Keep the changes history current while a book is open: the side panel and
  // the global undo/redo shortcuts both read this store, and imports or manual
  // edits move the data summary.
  $effect(() => {
    void dbSession.value;
    void dbSession.value?.summary.totalTransactions;
    void dbSession.value?.summary.totalAccounts;
    // The refresh reads and writes its own store synchronously (spread in
    // `patch`), which would otherwise be tracked here and re-trigger this
    // effect on every write — an infinite update loop.
    untrack(() => void refreshChangeHistory());
  });

  // Chrome is always mounted now; only the sidebar depends on having a book.
  // While book-less (welcome) the inset panel keeps its frame through the
  // explicit classes below, because Sidebar.Inset's peer-driven styles
  // (rounded corners, gutter) need a rendered Sidebar.Root to match.
  let showSidebar = $derived(!!dbSession.value);

  // Strict onboarding: any route other than the welcome page redirects home
  // while empty. `/about` stays reachable so the welcome footer's link works.
  // The welcome screen lives on `/`, so this never loops. Waits for the
  // bootstrap specifically (not refresh loading) so a home revalidation
  // never triggers a redirect.
  const booklessRoutes = new Set(['/', '/about']);
  $effect(() => {
    if (!dbBootstrapping.value && !dbSession.value && !booklessRoutes.has(page.url.pathname)) {
      goto(resolve('/'), { replaceState: true });
    }
  });

  // Effective theme ('system' resolves live via prefersDark) + document lang.
  let resolvedTheme = $derived(
    appSettings.value.theme === 'system'
      ? prefersDark.current
        ? 'dark'
        : 'light'
      : appSettings.value.theme
  );

  $effect(() => {
    document.documentElement.classList.toggle('dark', resolvedTheme === 'dark');
    document.documentElement.style.colorScheme = resolvedTheme;
    document.documentElement.lang = appSettings.value.locale;
    applyAccent(appSettings.value.accent, appSettings.value.customPrimary);
    applyDensity(appSettings.value.density);
  });

  function handleKeydown(e: KeyboardEvent) {
    // The palette must open from anywhere — including inside inputs — so
    // this chord is checked before the input guard below.
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      commandPalette.value = !commandPalette.value;
      return;
    }
    if (e.target instanceof HTMLElement && e.target.closest('input, textarea, [contenteditable]')) {
      return;
    }
    if (e.key === '?') {
      e.preventDefault();
      shortcutsDialog.value = !shortcutsDialog.value;
      return;
    }
    if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
      e.preventDefault();
      if (e.key === 'ArrowLeft') goBack();
      else goForward();
      return;
    }
    if (!(e.ctrlKey || e.metaKey)) return;
    if (e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if (e.shiftKey) void redoLatestChange();
      else void undoLatestChange();
      return;
    }
    if (e.key.toLowerCase() === 't' && e.shiftKey) {
      e.preventDefault();
      toggleTheme();
      return;
    }
    if (e.key.toLowerCase() === 'h') {
      e.preventDefault();
      toggleHistorySidebar();
      return;
    }
    if (e.key.toLowerCase() === 'b') {
      e.preventDefault();
      saveSettings({ sidebarCollapsed: !appSettings.value.sidebarCollapsed });
    }
  }
</script>

<svelte:document onkeydown={handleKeydown} />

<div class="[--header-height:calc(--spacing(10))]">
  <Sidebar.Provider class={cn('flex flex-col', !showSidebar && 'bg-sidebar')}>
    <TitleBar showSidebarTrigger={showSidebar} showHistoryToggle={showSidebar} />
    <div class="flex flex-1 overflow-hidden">
      {#if showSidebar}
        <AppSidebar />
      {/if}
      <Sidebar.Inset class={cn('min-w-0', !showSidebar && 'mx-2 mb-2 rounded-xl shadow-sm')}>
        <main id="main-content" class="flex flex-1 flex-col overflow-auto p-8">
          <div class="mx-auto flex w-full max-w-7xl flex-1 flex-col">
            {@render children()}
          </div>
        </main>
      </Sidebar.Inset>
      {#if showSidebar && historySidebar.value}
        <HistorySidebar />
      {/if}
    </div>
  </Sidebar.Provider>
</div>

<Sonner theme={resolvedTheme} />

<KeyboardShortcutsDialog />

<CommandPalette />
