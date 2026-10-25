<script lang="ts">
  import { goto, afterNavigate, onNavigate } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { untrack } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import { AppSidebar } from '$lib/components/layout/app-sidebar';
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
    loadActiveBook,
    refreshSession,
    activeBookPath
  } from '$lib/stores/db.svelte';
  import { NAV_CHORD_TARGETS } from '$lib/features/nav/nav-items';
  import { refreshNav } from '$lib/stores/nav.svelte';
  import { autoPostForBook } from '$lib/features/schedules/auto-post';
  import { TitleBar } from '$lib/components/layout/title-bar';
  import { cn } from '$lib/utils';
  import { Sonner, toast } from '$lib/components/ui/sonner';
  import KeyboardShortcutsDialog from '$lib/components/layout/keyboard-shortcuts-dialog.svelte';
  import GnuCashImportDialog from '$lib/features/book/gnucash-import-dialog.svelte';
  import AccountDialog from '$lib/features/accounts/account-dialog.svelte';
  import { accountDialog } from '$lib/features/accounts/account-dialog-state.svelte';
  import {
    keyboardShortcutsDialog as shortcutsDialog,
    commandPalette
  } from '$lib/stores/ui.svelte';
  import '../app.css';

  let { children } = $props();

  initNavHistory();

  // The inset main is the app’s only scroll container (the window can’t
  // scroll), so reset it on every navigation — detail and form pages should
  // open at the top even when the previous list was scrolled.
  afterNavigate(() => {
    document.getElementById('main-content')?.scrollTo({ top: 0 });
  });

  const prefersDark = new MediaQuery('(prefers-color-scheme: dark)');
  const prefersReducedMotion = new MediaQuery('(prefers-reduced-motion: reduce)');

  // Short cross-fade of the content column between pages (View Transitions;
  // see app.css). Same-page URL changes (filters, pagination) and reduced
  // motion skip it, as do webviews without the API.
  onNavigate((navigation) => {
    if (!document.startViewTransition || prefersReducedMotion.current) return;
    if (navigation.from?.url.pathname === navigation.to?.url.pathname) return;
    return new Promise((resolve) => {
      document.startViewTransition(async () => {
        resolve();
        await navigation.complete;
      });
    });
  });

  // Bootstrap DB state once so deep-linked/refreshed pages see real data
  // (e.g. Settings' "Clear all data" gate) instead of a stale null session.
  // The rejection is deliberately swallowed here: the store resets its
  // bootstrap flag, so a later navigation retries; pages that need the
  // session surface their own errors. The path loads afterwards: setting the
  // session bumps its sequence, which would discard a path fetched in parallel.
  $effect(() => {
    ensureDbSessionLoaded()
      .catch(() => {})
      .then(() => loadActiveBook());
  });

  // Keep sidebar roots + badges current while a book is open: they refresh on
  // navigation and whenever the data summary changes (mutations that return no
  // summary call `refreshSession()`, which forces a new summary).
  $effect(() => {
    void dbSession.value;
    void dbSession.value?.summary.totalTransactions;
    void dbSession.value?.summary.totalAccounts;
    void page.url.pathname;
    untrack(() => {
      void refreshNav();
    });
  });

  // Auto-post schedules catch up once per opened book.
  $effect(() => {
    const path = dbSession.value ? activeBookPath.value : null;
    untrack(() => void autoPostForBook(path));
  });

  // Chrome is always mounted now; only the sidebar depends on having a book.
  // While book-less (welcome) the inset panel keeps its frame through the
  // explicit classes below, because Sidebar.Inset’s peer-driven styles
  // (rounded corners, gutter) need a rendered Sidebar.Root to match.
  let showSidebar = $derived(!!dbSession.value);

  // Strict onboarding: any route other than the welcome page redirects home
  // while empty. `/about` stays reachable so the welcome footer’s link works.
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

  // `g`-prefix navigation chord: `g` arms the chord for a moment, the next
  // unmodified key resolves it (e.g. `g t` → Transactions).
  let pendingChord: ReturnType<typeof setTimeout> | null = null;

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
    if (pendingChord !== null) {
      clearTimeout(pendingChord);
      pendingChord = null;
      if (!e.ctrlKey && !e.metaKey && !e.altKey) {
        const target = NAV_CHORD_TARGETS[e.key.toLowerCase() as keyof typeof NAV_CHORD_TARGETS];
        if (target) {
          e.preventDefault();
          void goto(resolve(target));
          return;
        }
      }
    } else if (e.key.toLowerCase() === 'g' && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      pendingChord = setTimeout(() => (pendingChord = null), 800);
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
      toggleTheme();
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
    <TitleBar showSidebarTrigger={showSidebar} />
    <div class="flex flex-1 overflow-hidden">
      {#if showSidebar}
        <AppSidebar />
      {/if}
      <Sidebar.Inset class={cn('min-w-0', !showSidebar && 'mx-2 mb-2 rounded-xl shadow-sm')}>
        <main id="main-content" class="flex flex-1 flex-col overflow-auto p-(--page-padding)">
          <!-- `@container`: page grids respond to the content width, not the
               window, so the sidebar never leaves them cramped. -->
          <div class="@container mx-auto flex w-full max-w-7xl flex-1 flex-col">
            {@render children()}
          </div>
        </main>
      </Sidebar.Inset>
    </div>
  </Sidebar.Provider>
</div>

<Sonner theme={resolvedTheme} />

<KeyboardShortcutsDialog />

<GnuCashImportDialog />

<AccountDialog
  bind:open={accountDialog.value}
  mode="create"
  onSaved={() => {
    refreshSession();
    toast.success('Account created');
  }}
  onError={(msg) => toast.error(msg)}
/>
