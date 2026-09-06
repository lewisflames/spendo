<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { isTauri } from '@tauri-apps/api/core';
  import WindowControls from './window-controls.svelte';

  // Synchronous runtime check (prerender-safe); see window-controls.svelte.
  const inTauri = isTauri();
  let isMac = $state(false);

  $effect(() => {
    if (!inTauri) return;
    isMac = /Mac|iPhone|iPad/.test(navigator.userAgent);
  });

  function onHeaderMouseDown(e: MouseEvent) {
    if (!isMac || !inTauri) return;
    if (e.button !== 0) return;
    if ((e.target as Element).closest('button, a') !== null) return;
    getCurrentWindow().startDragging();
  }

  function onHeaderDoubleClick(e: MouseEvent) {
    if (!isMac || !inTauri) return;
    if ((e.target as Element).closest('button, a') !== null) return;
    getCurrentWindow().toggleMaximize();
  }
</script>

<!-- Chromeless shell for the welcome view: drag region + window controls only.
  Positioned above the fixed welcome background (which sits at z-0) while
  staying transparent so the grid shows through underneath. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<header
  class="relative z-10 flex h-10 shrink-0 items-center select-none"
  data-tauri-drag-region="true"
  ondblclick={onHeaderDoubleClick}
  onmousedown={onHeaderMouseDown}
>
  {#if inTauri && isMac}
    <div class="w-17.5 shrink-0"></div>
  {/if}
  <div class="min-w-0 flex-1"></div>
  <WindowControls />
</header>
