<script lang="ts">
  import { page } from '$app/state';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { isTauri } from '@tauri-apps/api/core';
  import { ArrowLeft, ArrowRight } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { SidebarTrigger } from '$lib/components/ui/sidebar/index.js';
  import { breadcrumb, type BreadcrumbItem } from '$lib/stores/breadcrumb.svelte';
  import { goBack, goForward, navHistory } from '$lib/stores/nav-history.svelte';
  import { routeTitle } from '$lib/routes';
  import * as Breadcrumb from '$lib/components/ui/breadcrumb';
  import WindowControls from './window-controls.svelte';

  // Synchronous runtime check (prerender-safe); see window-controls.svelte.
  const inTauri = isTauri();
  let isMac = $state(false);

  $effect(() => {
    if (!inTauri) return;
    isMac = /Mac|iPhone|iPad/.test(navigator.userAgent);
  });

  function isInteractive(e: MouseEvent) {
    return (e.target as Element).closest('button, a') !== null;
  }

  function onHeaderMouseDown(e: MouseEvent) {
    if (!isMac || !inTauri) return;
    if (e.button !== 0) return;
    if (isInteractive(e)) return;
    getCurrentWindow().startDragging();
  }

  function onHeaderDoubleClick(e: MouseEvent) {
    if (!isMac || !inTauri) return;
    if (isInteractive(e)) return;
    getCurrentWindow().toggleMaximize();
  }

  const defaultItems: BreadcrumbItem[] = $derived([{ name: routeTitle(page.url.pathname) }]);

  const items = $derived(
    breadcrumb.value.route === page.url.pathname ? breadcrumb.value.items : defaultItems
  );

  const canGoBack = $derived(navHistory.value.index > 0);
  const canGoForward = $derived(navHistory.value.index < navHistory.value.stack.length - 1);
</script>

<!-- Handlers implement Tauri window dragging/maximize, not document interaction. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<header
  class="flex h-10 shrink-0 items-center select-none"
  data-tauri-drag-region="true"
  ondblclick={onHeaderDoubleClick}
  onmousedown={onHeaderMouseDown}
>
  {#if inTauri && isMac}
    <div class="w-17.5 shrink-0"></div>
  {/if}

  <div class="flex h-full min-w-0 flex-1 items-center gap-2 px-3">
    <div class="flex shrink-0 items-center gap-0.5">
      <SidebarTrigger class="text-muted-foreground" />
      <Button
        variant="ghost"
        size="icon-sm"
        class="text-muted-foreground"
        aria-label="Go back"
        title="Go back"
        disabled={!canGoBack}
        onclick={goBack}
      >
        <ArrowLeft class="size-4" />
      </Button>
      <Button
        variant="ghost"
        size="icon-sm"
        class="text-muted-foreground"
        aria-label="Go forward"
        title="Go forward"
        disabled={!canGoForward}
        onclick={goForward}
      >
        <ArrowRight class="size-4" />
      </Button>
    </div>

    <Breadcrumb.Root>
      <Breadcrumb.List>
        {#each items as item, i (i)}
          <Breadcrumb.Item>
            {#if item.href && i < items.length - 1}
              <Breadcrumb.Link href={item.href}>{item.name}</Breadcrumb.Link>
            {:else}
              <Breadcrumb.Page>{item.name}</Breadcrumb.Page>
            {/if}
          </Breadcrumb.Item>
          {#if i < items.length - 1}
            <Breadcrumb.Separator />
          {/if}
        {/each}
      </Breadcrumb.List>
    </Breadcrumb.Root>
  </div>

  <WindowControls />
</header>
