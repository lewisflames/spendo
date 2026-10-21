/**
 * Small per-viewer UI state (a collapsed panel, an expanded tree) persisted in
 * localStorage under a stable key. Invalid or unreadable stored values fall
 * back to the default; storage failures keep the state memory-only.
 */
export function usePersistedState<T>(
  storageKey: string,
  fallback: T,
  isValid: (v: unknown) => v is T
): { value: T } {
  let value = $state<T>(load());

  function load(): T {
    try {
      const raw = localStorage.getItem(storageKey);
      if (raw === null) return fallback;
      const parsed: unknown = JSON.parse(raw);
      return isValid(parsed) ? parsed : fallback;
    } catch {
      return fallback;
    }
  }

  return {
    get value() {
      return value;
    },
    set value(next: T) {
      value = next;
      try {
        localStorage.setItem(storageKey, JSON.stringify(next));
      } catch {
        // Storage full/blocked — state stays memory-only.
      }
    }
  };
}

export const isBoolean = (v: unknown): v is boolean => typeof v === 'boolean';

export const isStringArray = (v: unknown): v is string[] =>
  Array.isArray(v) && v.every((item) => typeof item === 'string');
