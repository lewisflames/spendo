<script lang="ts">
  import { tooltip } from '$lib/attachments/tooltip';
  import { ArrowLeft, ArrowRight } from '@lucide/svelte';
  import { Button } from '$lib/components/ui/button';
  import { SidebarTrigger } from '$lib/components/ui/sidebar/index.js';
  import { goBack, goForward, navHistory } from '$lib/stores/nav-history.svelte';
  import { useWindowDrag } from '$lib/hooks/use-window-drag.svelte';
  import CommandPalette from './command-palette.svelte';
  import WindowControls from './window-controls.svelte';

  let {
    showSidebarTrigger = true
  }: {
    showSidebarTrigger?: boolean;
  } = $props();

  const {
    inTauri,
    isMac,
    onMouseDown: onHeaderMouseDown,
    onDoubleClick: onHeaderDoubleClick
  } = useWindowDrag();

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

  <div class="flex h-full shrink-0 items-center gap-0.5 pl-3">
    {#if showSidebarTrigger}
      <SidebarTrigger class="text-muted-foreground" />
    {/if}
    <Button
      variant="ghost"
      size="icon-sm"
      class="text-muted-foreground"
      aria-label="Go back"
      {@attach tooltip('Go back')}
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
      {@attach tooltip('Go forward')}
      disabled={!canGoForward}
      onclick={goForward}
    >
      <ArrowRight class="size-4" />
    </Button>
  </div>

  <div class="flex h-full min-w-0 flex-1 items-center justify-center px-3">
    <CommandPalette />
  </div>

  <WindowControls />
</header>
