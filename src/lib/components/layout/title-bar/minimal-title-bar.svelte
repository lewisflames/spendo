<script lang="ts">
  import { useWindowDrag } from '$lib/hooks/use-window-drag.svelte';
  import WindowControls from './window-controls.svelte';

  const {
    inTauri,
    isMac,
    onMouseDown: onHeaderMouseDown,
    onDoubleClick: onHeaderDoubleClick
  } = useWindowDrag();
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
