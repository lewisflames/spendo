import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';

/**
 * OS file-drop events at the webview level (Tauri swallows HTML5 drop events).
 * `dragging` stays true from `enter` to `drop`/`leave` so callers can show a
 * preview; paths are captured on `enter` because `over` omits them.
 */
export function useFileDrop(onDrop: (paths: string[]) => void) {
  const state = $state<{ dragging: boolean; paths: string[] }>({
    dragging: false,
    paths: []
  });

  $effect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | null = null;
    let disposed = false;

    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === 'enter') {
          state.dragging = true;
          state.paths = payload.paths;
        } else if (payload.type === 'over') {
          state.dragging = true;
        } else if (payload.type === 'drop') {
          state.dragging = false;
          state.paths = [];
          onDrop(payload.paths);
        } else {
          state.dragging = false;
          state.paths = [];
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  return {
    get dragging() {
      return state.dragging;
    },
    get paths() {
      return state.paths;
    }
  };
}
