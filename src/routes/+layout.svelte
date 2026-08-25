<script lang="ts">
  import { onNavigate } from '$app/navigation';
  import { MediaQuery } from 'svelte/reactivity';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import { AppSidebar } from '$lib/components/layout/app-sidebar';
  import { appSettings, saveSettings, resolveTheme } from '$lib/stores/settings.svelte';
  import { goBack, goForward, initNavHistory } from '$lib/stores/nav-history.svelte';
  import { ensureDbSessionLoaded } from '$lib/stores/db.svelte';
  import { TitleBar } from '$lib/components/layout/title-bar';
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
  <Sidebar.Provider class="flex flex-col">
    <TitleBar />
    <div class="flex flex-1 overflow-hidden">
      <AppSidebar />
      <Sidebar.Inset class="min-w-0">
        <main id="main-content" class="flex-1 overflow-auto p-8">
          {@render children()}
        </main>
      </Sidebar.Inset>
    </div>
  </Sidebar.Provider>
</div>

<Sonner theme={resolvedTheme} />

<KeyboardShortcutsDialog />
