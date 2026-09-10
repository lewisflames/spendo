import { errMsg } from '$lib/utils/error';

interface AsyncDataConfig<T> {
  /** Whether fetching should happen (e.g. a DB session exists). */
  enabled: () => boolean;
  /**
   * The backend invocation. Called inside an effect, so reactive reads here
   * are tracked and re-trigger the fetch. Must return a fresh promise.
   */
  fetch: () => Promise<T>;
  /**
   * Runs with the resolved value after built-in state is updated (only when
   * the result was not superseded). Use for post-processing such as
   * expanding tree nodes.
   */
  onSuccess?: (data: T) => void;
  /**
   * Extra failure handling (e.g. a toast) for pages that don't surface the
   * built-in `error` state inline.
   */
  onError?: (message: string) => void;
  /** Fallback message used by errMsg() on failure. */
  label: string;
}

/**
 * Effect-scoped async fetch with cancellation, replacing the repeated
 * `let cancelled = false; …; return () => { cancelled = true }` boilerplate.
 * Re-runs whenever the reactive state read by `enabled`/`fetch` changes;
 * stale results are dropped. Previous data is kept while refetching.
 */
export function useAsyncData<T>({ enabled, fetch, onSuccess, onError, label }: AsyncDataConfig<T>) {
  let data = $state<T | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let reloadSeq = $state(0);

  $effect(() => {
    if (!enabled()) {
      data = null;
      loading = false;
      error = null;
      return;
    }

    let cancelled = false;
    void reloadSeq;
    loading = true;
    error = null;

    void (async () => {
      let result: T;
      try {
        result = await fetch();
      } catch (e) {
        if (cancelled) return;
        error = errMsg(e, label);
        loading = false;
        onError?.(error);
        return;
      }
      if (cancelled) return;
      data = result;
      loading = false;
      onSuccess?.(result);
    })();

    return () => {
      cancelled = true;
    };
  });

  return {
    get data(): T | null {
      return data;
    },
    get loading(): boolean {
      return loading;
    },
    get error(): string | null {
      return error;
    },
    reload(): void {
      reloadSeq++;
    }
  };
}
