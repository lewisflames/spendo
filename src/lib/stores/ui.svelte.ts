/** Store convention: `$state()` wrapped in `{ value }` like every other store. */

/** Global command palette visibility (layout hotkeys + title-bar search bar). */
export const commandPalette = $state({ value: false });

/** Keyboard shortcuts dialog visibility (sidebar + palette command). */
export const keyboardShortcutsDialog = $state({ value: false });

/** Close all overlay dialogs — called on every navigation. */
export function closeAllOverlays() {
  commandPalette.value = false;
  keyboardShortcutsDialog.value = false;
}
