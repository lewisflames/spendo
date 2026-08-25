/**
 * Tri-state table sort state: clicking the active column flips direction;
 * a new column sorts ascending for text keys, descending for numeric ones
 * (the conventional default for financial tables).
 */
export function useTableSort<K extends string>(
  defaults: { sortBy: K; sortDir: 'asc' | 'desc' },
  /** Keys whose first click sorts descending instead of ascending. */
  descFirst: readonly K[] = []
): {
  sortBy: K;
  sortDir: 'asc' | 'desc';
  toggle: (key: K) => void;
} {
  let sortBy = $state<K>(defaults.sortBy);
  let sortDir = $state<'asc' | 'desc'>(defaults.sortDir);

  return {
    get sortBy() {
      return sortBy;
    },
    get sortDir() {
      return sortDir;
    },
    toggle(key: K) {
      if (sortBy === key) {
        sortDir = sortDir === 'asc' ? 'desc' : 'asc';
      } else {
        sortBy = key;
        sortDir = descFirst.includes(key) ? 'desc' : 'asc';
      }
    }
  };
}
