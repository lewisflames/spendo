import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isMacPlatform } from '$lib/utils/platform';

/**
 * Tauri frameless-window handlers shared by the full and minimal title bars:
 * macOS drags the window from non-interactive header areas, and a
 * double-click there toggles maximize.
 */
export function useWindowDrag() {
  const inTauri = isTauri();
  const isMac = isMacPlatform();

  function isInteractive(e: MouseEvent): boolean {
    return (e.target as Element).closest('button, a') !== null;
  }

  function onMouseDown(e: MouseEvent): void {
    if (!isMac || !inTauri || e.button !== 0 || isInteractive(e)) return;
    getCurrentWindow().startDragging();
  }

  function onDoubleClick(e: MouseEvent): void {
    if (!isMac || !inTauri || isInteractive(e)) return;
    getCurrentWindow().toggleMaximize();
  }

  return { inTauri, isMac, onMouseDown, onDoubleClick };
}
