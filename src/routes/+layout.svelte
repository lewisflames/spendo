<script lang="ts">
  import { onMount } from 'svelte';
  import { Minus, Square, X } from '@lucide/svelte';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import AppSidebar from '$lib/components/app-sidebar.svelte';
  import { appSettings, saveSettings } from '$lib/stores/settings.svelte';
  import { IsMobile } from '$lib/hooks/IsMobile.svelte';
  import { SIDEBAR_COOKIE_NAME } from '$lib/components/ui/sidebar/constants.js';
  import './layout.css';

  let { children } = $props();

  let isSmallScreen = new IsMobile(1280);
  // eslint-disable-next-line svelte/prefer-writable-derived -- bound to Sidebar.Provider, must be writable
  let sidebarOpen = $state(false);

  onMount(() => {
    const cookie = document.cookie.split('; ').find((c) => c.startsWith(`${SIDEBAR_COOKIE_NAME}=`));
    sidebarOpen = cookie ? cookie.split('=')[1] === 'true' : !isSmallScreen.current;
  });

  $effect(() => {
    sidebarOpen = !isSmallScreen.current;
  });

  $effect(() => {
    const theme = appSettings.value.theme;

    document.documentElement.classList.toggle('dark', theme === 'dark');
    document.documentElement.style.colorScheme = theme;
  });

  function handleKeydown(e: KeyboardEvent) {
    if (!(e.ctrlKey || e.metaKey)) return;
    if (e.key.toLowerCase() === 't') {
      e.preventDefault();
      saveSettings({ theme: appSettings.value.theme === 'light' ? 'dark' : 'light' });
    }
  }
</script>

<svelte:document onkeydown={handleKeydown} />

<Sidebar.Provider bind:open={sidebarOpen}>
  <AppSidebar />
  <div class="flex flex-1 flex-col">
    <header
      class="flex h-9.5 min-h-9.5 select-none items-center border-b bg-background px-2"
      style="app-region: drag; -webkit-app-region: drag"
    >
      <div style="app-region: no-drag; -webkit-app-region: no-drag">
        <Sidebar.Trigger />
      </div>

      <div class="flex-1"></div>

      <div
        class="max-sm:hidden flex items-center gap-0.5"
        style="app-region: no-drag; -webkit-app-region: no-drag"
      >
        <button
          type="button"
          tabindex="-1"
          class="inline-flex size-7.5 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
          aria-label="Minimize"
        >
          <Minus aria-hidden="true" class="size-3.5" />
        </button>
        <button
          type="button"
          tabindex="-1"
          class="inline-flex size-7.5 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
          aria-label="Maximize"
        >
          <Square aria-hidden="true" class="size-3" />
        </button>
        <button
          type="button"
          tabindex="-1"
          class="inline-flex size-7.5 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive hover:text-destructive-foreground"
          aria-label="Close"
        >
          <X aria-hidden="true" class="size-3.5" />
        </button>
      </div>
    </header>

    <main class="flex-1 overflow-auto p-2 sm:p-3">
      <div class="min-h-full rounded-lg border bg-card p-4 shadow-sm sm:p-6">
        {@render children()}
      </div>
    </main>
  </div>
</Sidebar.Provider>
