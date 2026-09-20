/** Store convention: `$state()` wrapped in `{ value }` like every other store. */

/** Global command palette visibility (layout hotkeys + title-bar search bar). */
export const commandPalette = $state({ value: false });

/** Keyboard shortcuts dialog visibility (sidebar + palette command). */
export const keyboardShortcutsDialog = $state({ value: false });

/**
 * Changes-history side panel visibility (title-bar toggle + palette command).
 * Session-only: not part of the persisted settings store, and deliberately
 * kept out of `closeAllOverlays` so navigation never closes the panel.
 */
export const historySidebar = $state({ value: false });

/** Toggle the changes-history panel. */
export function toggleHistorySidebar() {
  historySidebar.value = !historySidebar.value;
}

/** Close all overlay dialogs — called on every navigation. */
export function closeAllOverlays() {
  commandPalette.value = false;
  keyboardShortcutsDialog.value = false;
}
