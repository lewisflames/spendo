import { untrack } from 'svelte';
import { page } from '$app/state';
import { goto } from '$app/navigation';

export interface UrlParamSpec {
  key: string;
  get: () => string | null;
  apply: (value: string | null) => void;
  /** Whether changes to this param should replace history instead of pushing. */
  replace?: boolean;
}

/** Spec factory: a 1-based page number that stays valid when absent. */
export function pageParam(
  key: string,
  getPage: () => number,
  setPage: (p: number) => void
): UrlParamSpec {
  return {
    key,
    get: () => String(getPage()),
    apply: (v) => {
      const n = Number(v);
      setPage(Number.isInteger(n) && n >= 1 ? n : 1);
    }
  };
}

/**
 * Spec factory: debounced search. The caller applies the value to both the
 * raw input (what's in the field) and the committed search (what's fetched).
 */
export function debouncedSearchParam(
  key: string,
  getCommitted: () => string | null,
  applyValue: (v: string) => void
): UrlParamSpec {
  return {
    key,
    get: getCommitted,
    apply: (v) => applyValue(v ?? ''),
    replace: true
  };
}

/** Spec factory for an optional `YYYY-MM-DD` date bound to a picker. */
export function dateParam(
  key: string,
  get: () => string | null,
  set: (v: string | null) => void
): UrlParamSpec {
  return { key, get, apply: (v) => set(v || null) };
}

/**
 * Two-way sync between component state and the URL query string, following the
 * pattern used by the Budgets page. Call from a component (before data-loading
 * effects) with one spec per param:
 *
 * - The read effect applies URL params to state whenever the URL changes,
 *   so initial loads and back/forward navigation restore the view.
 * - The write effect pushes (or replaces) the URL when state changes,
 *   guarded against loops by comparing serialized queries.
 */
export function useUrlParams(specs: UrlParamSpec[]) {
  // Apply URL params to state (runs before data-loading effects)
  $effect(() => {
    const sp = page.url.searchParams;
    // untrack: apply callbacks may read reactive state (e.g. to keep the
    // untouched side of a range); without this, the effect would depend on
    // the very state it writes and loop forever.
    untrack(() => {
      for (const spec of specs) {
        spec.apply(sp.get(spec.key));
      }
    });
  });

  // Push state changes to the URL
  $effect(() => {
    const sp = page.url.searchParams;
    const changedSpecs = specs.filter((s) => (s.get() ?? '') !== (sp.get(s.key) ?? ''));
    if (changedSpecs.length === 0) return;

    // Rebuild the query from plain strings. Deliberately NOT a (Svelte)URLSearchParams:
    // mutating a reactive params object inside this effect would re-trigger it
    // before the async goto lands, looping forever.
    const parts: string[] = [];
    for (const [k, v] of sp.entries()) {
      if (specs.some((s) => s.key === k)) continue;
      parts.push(`${encodeURIComponent(k)}=${encodeURIComponent(v)}`);
    }
    for (const s of specs) {
      const value = s.get() ?? '';
      if (value !== '') parts.push(`${encodeURIComponent(s.key)}=${encodeURIComponent(value)}`);
    }
    const query = parts.join('&');
    const url = query ? `${page.url.pathname}?${query}` : page.url.pathname;
    // Reconstructs the current route's URL (query-only sync), so resolve()
    // doesn't apply.
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    goto(url, {
      replaceState: changedSpecs.every((s) => s.replace === true),
      noScroll: true,
      keepFocus: true
    });
  });
}
