import { afterNavigate } from '$app/navigation';
import { closeAllOverlays } from '$lib/stores/ui.svelte';

interface NavHistoryState {
  stack: string[];
  index: number;
}

export const navHistory = $state<{ value: NavHistoryState }>({
  value: { stack: [], index: -1 }
});

const MAX_STACK = 200;

/**
 * Tracks in-app navigations for the title-bar back/forward buttons.
 * Limitation: a deep link starts with `stack=[href], index=0`, so Back is
 * disabled even when the browser itself could go back (to an external page).
 * This mirrors in-app history only by design — like a browser, going back
 * past the first in-app entry would leave the app.
 */
export function initNavHistory() {
  afterNavigate(({ to, type, delta }) => {
    closeAllOverlays();
    if (!to) return;
    const href = to.url.href;

    if (navHistory.value.index === -1) {
      navHistory.value.stack = [href];
      navHistory.value.index = 0;
      return;
    }

    if (type === 'popstate') {
      const target = Math.min(
        Math.max(navHistory.value.index + (delta ?? 0), 0),
        navHistory.value.stack.length - 1
      );
      if (navHistory.value.stack[target] === href) {
        navHistory.value.index = target;
        return;
      }
      const idx = navHistory.value.stack.indexOf(href);
      if (idx !== -1) {
        navHistory.value.index = idx;
        return;
      }
    }

    if (navHistory.value.stack[navHistory.value.index] === href) return;

    navHistory.value.stack = [...navHistory.value.stack.slice(0, navHistory.value.index + 1), href];
    // Trim old entries so a long session can't grow the stack unboundedly.
    if (navHistory.value.stack.length > MAX_STACK) {
      navHistory.value.stack = navHistory.value.stack.slice(
        navHistory.value.stack.length - MAX_STACK
      );
      navHistory.value.index = navHistory.value.stack.length - 1;
      return;
    }
    navHistory.value.index += 1;
  });
}

export function goBack() {
  if (navHistory.value.index > 0) window.history.back();
}

export function goForward() {
  if (navHistory.value.index < navHistory.value.stack.length - 1) window.history.forward();
}
