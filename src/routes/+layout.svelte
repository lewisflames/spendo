<script lang="ts">
  import { onNavigate, goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { MediaQuery } from 'svelte/reactivity';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import { AppSidebar } from '$lib/components/layout/app-sidebar';
  import { appSettings, saveSettings, resolveTheme } from '$lib/stores/settings.svelte';
  import { goBack, goForward, initNavHistory } from '$lib/stores/nav-history.svelte';
  import {
    dbSession,
    dbSessionLoading,
    ensureDbSessionLoaded,
    loadActiveBook
  } from '$lib/stores/db.svelte';
  import { TitleBar, MinimalTitleBar } from '$lib/components/layout/title-bar';
  import { Sonner } from '$lib/components/ui/sonner';
  import KeyboardShortcutsDialog from '$lib/components/layout/keyboard-shortcuts-dialog.svelte';
  import { keyboardShortcutsDialog as shortcutsDialog } from '$lib/stores/keyboard-shortcuts.svelte';
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

  // Chromeless welcome shell while there is no data (first start, or after
  // Settings → Clear all data). During the initial bootstrap we also stay
  // chromeless so neither shell flashes for the wrong audience.
  //
  // NOTE: this must depend on session identity ALONE, never on
  // dbSessionLoading. The home page force-refreshes the session on every
  // mount, which toggles loading; if the shell read loading, mounting the
  // app shell would hide it again → remount → refetch → an infinite
  // shell-flapping fetch storm that freezes the app.
  let showAppChrome = $derived(!!dbSession.value);

  // Strict onboarding: any route other than `/` redirects home while empty.
  // The welcome screen lives on `/`, so this never loops.
  $effect(() => {
    if (!dbSessionLoading.value && !dbSession.value && page.url.pathname !== '/') {
      goto(resolve('/'), { replaceState: true });
    }
  });

  let transitionInFlight = false;

  onNavigate((navigation) => {
    if (!document.startViewTransition) return;
    // User preference wins; otherwise honor the OS reduce-motion setting.
    if (appSettings.value.reduceMotion) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    // A URL-sync goto can start a second navigation while the first
    // transition is still running; starting another one skips/aborts it.
    if (transitionInFlight) return;
    transitionInFlight = true;
    return new Promise((resolve) => {
      const transition = document.startViewTransition(async () => {
        // Resolve early so SvelteKit can render; the callback must stay alive
        // until then or the browser would snapshot an unchanged DOM.
        resolve();
        try {
          await navigation.complete;
        } catch {
          // aborted by a newer navigation (e.g. URL-sync goto) — ignore
        }
      });
      // A superseded transition rejects ready/finished with AbortError;
      // swallow those so they never surface as unhandled rejections.
      transition.updateCallbackDone.catch(() => {});
      transition.ready.catch(() => {});
      transition.finished
        .catch(() => {})
        .finally(() => {
          transitionInFlight = false;
        });
    });
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
    // Mirrors Settings → Appearance → Reduce motion onto <html> for
    // CSS-driven ambient decoration.
    document.documentElement.classList.toggle('reduce-motion', appSettings.value.reduceMotion);
  });

  function handleKeydown(e: KeyboardEvent) {
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
