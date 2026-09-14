/**
 * Column visibility state shared by tables with a columns menu
 * (account ledger, holdings). Preserves `COLUMNS` order on show.
 * Pass a stable `storageKey` (e.g. `cols:ledger`) to persist across
 * mounts; otherwise state is memory-only.
 */
export interface ColumnVisibility<Id extends string> {
  readonly visibleColumns: Id[];
  isVisible: (id: Id) => boolean;
  setVisible: (id: Id, checked: boolean, allIds?: Id[]) => void;
  resetColumns: () => void;
}

function loadStored<Id extends string>(key: string, defaults: Id[]): Id[] | null {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return null;
    // Keep only known ids, preserve defaults order.
    const set = new Set(parsed.filter((v): v is Id => typeof v === 'string'));
    const ordered = defaults.filter((d) => set.has(d));
    return ordered.length > 0 ? ordered : null;
  } catch {
    return null;
  }
}

export function useColumnVisibility<Id extends string>(
  defaults: Id[],
  storageKey?: string
): ColumnVisibility<Id> {
  let visibleColumns = $state<Id[]>([...defaults]);

  if (storageKey) {
    const stored = typeof localStorage !== 'undefined' ? loadStored(storageKey, defaults) : null;
    if (stored) visibleColumns = stored;
  }

  function persist(next: Id[]) {
    if (!storageKey) return;
    try {
      localStorage.setItem(storageKey, JSON.stringify(next));
    } catch {
      // Storage full/blocked — visibility stays memory-only.
    }
  }

  function isVisible(id: Id): boolean {
    return visibleColumns.includes(id);
  }

  function setVisible(id: Id, checked: boolean, allIds?: Id[]): void {
    let next: Id[];
    if (checked) {
      if (visibleColumns.includes(id)) return;
      const order = allIds ?? [...visibleColumns, id];
      next = order.filter((c) => c === id || visibleColumns.includes(c));
    } else {
      if (!visibleColumns.includes(id)) return;
      next = visibleColumns.filter((c) => c !== id);
      // Never persist an empty set — keep at least one column visible.
      if (next.length === 0) return;
    }
    visibleColumns = next;
    persist(next);
  }

  function resetColumns(): void {
    visibleColumns = [...defaults];
    persist([...defaults]);
  }

  return {
    get visibleColumns(): Id[] {
      return visibleColumns;
    },
    isVisible,
    setVisible,
    resetColumns
  };
}
