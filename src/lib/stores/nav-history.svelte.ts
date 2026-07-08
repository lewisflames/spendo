import { afterNavigate } from '$app/navigation';

export interface NavHistoryState {
  stack: string[];
  index: number;
}

export const navHistory = $state<{ value: NavHistoryState }>({
  value: { stack: [], index: -1 }
});

const MAX_STACK = 200;

export function initNavHistory() {
  afterNavigate(({ to, type, delta }) => {
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
