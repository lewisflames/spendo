<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { MediaQuery } from 'svelte/reactivity';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import { AppSidebar } from '$lib/components/layout/app-sidebar';
  import {
    appSettings,
    saveSettings,
    applyAccent,
    applyDensity,
    resolveTheme
  } from '$lib/stores/settings.svelte';
  import { goBack, goForward, initNavHistory } from '$lib/stores/nav-history.svelte';
  import {
    dbSession,
    dbBootstrapping,
    ensureDbSessionLoaded,
    loadActiveBook
  } from '$lib/stores/db.svelte';
  import { TitleBar, MinimalTitleBar } from '$lib/components/layout/title-bar';
  import { Sonner } from '$lib/components/ui/sonner';
  import KeyboardShortcutsDialog from '$lib/components/layout/keyboard-shortcuts-dialog.svelte';
  import CommandPalette from '$lib/components/layout/command-palette.svelte';
  import {
    keyboardShortcutsDialog as shortcutsDialog,
    commandPalette
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

  // App chrome stays mounted while the first bootstrap is in flight so a
  // full reload paints TitleBar + Sidebar around the page skeletons instead
  // of dropping to the chromeless welcome shell mid-load. Once bootstrap
  // settles, session identity alone decides (empty → welcome shell).
  //
  // NOTE: this reads dbBootstrapping, never dbSessionLoading. The home page
  // force-refreshes the session on every mount, which toggles loading with
  // the session intact; bootstrapping is already false by then, so refreshes
  // can't flap the shell into a remount → refetch loop. Tradeoff: a true
  // first run (or cleared book) briefly shows the app shell with skeletons
  // before settling on welcome — preferred over every reload flashing
  // chromeless, since a persisted hint proved unreliable across webview /
  // browser storage contexts.
  let showAppChrome = $derived(!!dbSession.value || dbBootstrapping.value);

  // Strict onboarding: any route other than `/` redirects home while empty.
  // The welcome screen lives on `/`, so this never loops. Waits for the
  // bootstrap specifically (not refresh loading) so a home revalidation
  // never triggers a redirect.
  $effect(() => {
    if (!dbBootstrapping.value && !dbSession.value && page.url.pathname !== '/') {
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
    if (e.key.toLowerCase() === 't' && e.shiftKey) {
      e.preventDefault();
      // Toggling always lands on an explicit theme, leaving 'system' mode.
      saveSettings({ theme: resolveTheme(appSettings.value.theme) === 'dark' ? 'light' : 'dark' });
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
  {#if showAppChrome}
    <Sidebar.Provider class="flex flex-col">
      <TitleBar />
      <div class="flex flex-1 overflow-hidden">
        <AppSidebar />
        <Sidebar.Inset class="min-w-0">
          <main id="main-content" class="flex-1 overflow-auto p-8">
            <div class="mx-auto w-full max-w-7xl">
              {@render children()}
            </div>
          </main>
        </Sidebar.Inset>
      </div>
    </Sidebar.Provider>
  {:else}
    <div class="flex h-svh flex-col">
      <MinimalTitleBar />
      <main id="main-content" class="relative flex flex-1 flex-col overflow-auto">
        {@render children()}
      </main>
    </div>
  {/if}
</div>

<Sonner theme={resolvedTheme} />

<KeyboardShortcutsDialog />

<CommandPalette />
