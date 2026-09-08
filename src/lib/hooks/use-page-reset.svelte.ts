const UNSET: unique symbol = Symbol('unset');

/**
 * Run `reset` (typically `page = 1`) whenever `key()` changes, skipping the
 * first run so deep-linked pages survive mount. Centralizes the `prevKey`
 * plain-`let`-in-`$effect` pattern, which is fragile and silently depends on
 * declaration order.
 *
 * Invoke BEFORE `useAsyncData` so the reset runs first in the same flush and
 * the fetch sees the reset page immediately instead of firing a doomed
 * request. `key` must return a value comparable with `Object.is` — a string
 * (e.g. `JSON.stringify(deps)`) or a stable reference. Never return a fresh
 * object/array (new identity every run would reset on every flush).
 */
export function useResetPageOnChange(key: () => unknown, reset: () => void): void {
  let prev: unknown = UNSET;
  $effect(() => {
    const k = key();
    if (prev !== UNSET && !Object.is(prev, k)) reset();
    prev = k;
  });
}
