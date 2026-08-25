<script lang="ts">
  import { page } from '$app/state';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { ArrowLeft, ArrowRight, Minus, Square, Copy, X } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { SidebarTrigger } from '$lib/components/ui/sidebar/index.js';
  import { breadcrumb, type BreadcrumbItem } from '$lib/stores/breadcrumb.svelte';
  import { goBack, goForward, navHistory } from '$lib/stores/nav-history.svelte';
  import { routeTitle } from '$lib/routes';
  import * as Breadcrumb from '$lib/components/ui/breadcrumb';

  let maximized = $state(false);
  let isTauri = $state(false);
  let isMac = $state(false);

  $effect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;

    try {
      const win = getCurrentWindow();
      isTauri = true;
      isMac = /Mac|iPhone|iPad/.test(navigator.userAgent);
      win.isMaximized().then((m) => {
        maximized = m;
      });
      win
        .onResized(() => {
          win.isMaximized().then((m) => {
            maximized = m;
          });
        })
        .then((fn) => {
          // The effect may already have been torn down before the promise
          // resolved; clean up immediately instead of leaking the listener.
          if (disposed) fn();
          else unlisten = fn;
        });
    } catch {
      // not running in Tauri (e.g. browser dev)
    }

    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  function minimize() {
    getCurrentWindow().minimize();
  }

  function toggleMaximize() {
    getCurrentWindow().toggleMaximize();
  }

  function closeWindow() {
    getCurrentWindow().close();
  }

  function isInteractive(e: MouseEvent) {
    return (e.target as Element).closest('button, a') !== null;
  }

  function onHeaderMouseDown(e: MouseEvent) {
    if (!isMac || !isTauri) return;
    if (e.button !== 0) return;
    if (isInteractive(e)) return;
    getCurrentWindow().startDragging();
  }

  function onHeaderDoubleClick(e: MouseEvent) {
    if (!isMac || !isTauri) return;
    if (isInteractive(e)) return;
    toggleMaximize();
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
  {#if isTauri && isMac}
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

  {#if isTauri && !isMac}
    <div class="flex h-full items-stretch">
      <button
        onclick={minimize}
        class="group flex h-full w-9 items-center justify-center"
        aria-label="Minimize"
        type="button"
      >
        <span
          class="flex size-6 items-center justify-center rounded-full group-hover:bg-muted/80 group-active:bg-muted"
        >
          <Minus class="size-3.5" />
        </span>
      </button>
      <button
        onclick={toggleMaximize}
        class="group flex h-full w-9 items-center justify-center"
        aria-label={maximized ? 'Restore' : 'Maximize'}
        type="button"
      >
        <span
          class="flex size-6 items-center justify-center rounded-full group-hover:bg-muted/80 group-active:bg-muted"
        >
          {#if maximized}
            <Copy class="size-3" />
          {:else}
            <Square class="size-3" />
          {/if}
        </span>
      </button>
      <button
        onclick={closeWindow}
        class="group flex h-full w-9 items-center justify-center"
        aria-label="Close"
        type="button"
      >
        <span
          class="group-hover:text-destructive-foreground flex size-6 items-center justify-center rounded-full group-hover:bg-destructive/80 group-active:bg-destructive"
        >
          <X class="size-3.5" />
        </span>
      </button>
    </div>
  {/if}
</header>
