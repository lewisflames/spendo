<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { isTauri } from '@tauri-apps/api/core';
  import { Minus, Square, Copy, X } from '@lucide/svelte';

  // Synchronous runtime check (prerender-safe). getCurrentWindow() never
  // throws, so try/catch can’t tell plain-browser dev apart from Tauri —
  // without this gate every IPC below fails in the browser with console spam.
  const inTauri = isTauri();

  let maximized = $state(false);
  let isMac = $state(false);

  $effect(() => {
    if (!inTauri) return;
    isMac = /Mac|iPhone|iPad/.test(navigator.userAgent);

    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    let disposed = false;

    win
      .isMaximized()
      .then((m) => {
        maximized = m;
      })
      .catch(() => {});
    win
      .onResized(() => {
        win
          .isMaximized()
          .then((m) => {
            maximized = m;
          })
          .catch(() => {});
      })
      .then((fn) => {
        // The effect may already have been torn down before the promise
        // resolved; clean up immediately instead of leaking the listener.
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {});

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
</script>

{#if inTauri && !isMac}
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
