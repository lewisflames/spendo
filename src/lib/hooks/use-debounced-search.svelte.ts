export function useDebouncedSearch<T = string>(
  input: () => string,
  transform: (value: string) => T = (v) => v as T,
  delay = 300
): { value: T; set: (value: string) => void } {
  let committed = $state<T>(transform(''));
  let pendingTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    const val = input();
    const timer = setTimeout(() => {
      pendingTimer = null;
      committed = transform(val);
    }, delay);
    pendingTimer = timer;
    return () => {
      clearTimeout(timer);
      if (pendingTimer === timer) pendingTimer = null;
    };
  });

  return {
    get value() {
      return committed;
    },
    set(value: string) {
      // Commit immediately and drop any in-flight debounce timer so it can’t
      // re-commit a stale transform of the raw input later.
      if (pendingTimer !== null) {
        clearTimeout(pendingTimer);
        pendingTimer = null;
      }
      committed = transform(value);
    }
  };
}
