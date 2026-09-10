import { browser } from '$app/environment';

/** True on Apple platforms (SSR-safe). */
export function isMacPlatform(): boolean {
  return browser && /Mac|iPhone|iPad/.test(navigator.userAgent);
}

/** Label for the primary modifier key in shortcut hints. */
export function modKeyLabel(): string {
  return isMacPlatform() ? '⌘' : 'Ctrl';
}
