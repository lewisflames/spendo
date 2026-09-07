/**
 * Column visibility state shared by tables with a columns menu
 * (account ledger, holdings). Preserves `COLUMNS` order on show.
 */
export interface ColumnVisibility<Id extends string> {
  readonly visibleColumns: Id[];
  isVisible: (id: Id) => boolean;
  setVisible: (id: Id, checked: boolean, allIds?: Id[]) => void;
  resetColumns: () => void;
}

export function useColumnVisibility<Id extends string>(defaults: Id[]): ColumnVisibility<Id> {
  let visibleColumns = $state<Id[]>([...defaults]);

  function isVisible(id: Id): boolean {
    return visibleColumns.includes(id);
  }

  function setVisible(id: Id, checked: boolean, allIds?: Id[]): void {
    if (checked) {
      if (!visibleColumns.includes(id)) {
        const order = allIds ?? [...visibleColumns, id];
        visibleColumns = order.filter((c) => c === id || visibleColumns.includes(c));
      }
    } else {
      visibleColumns = visibleColumns.filter((c) => c !== id);
    }
  }

  function resetColumns(): void {
    visibleColumns = [...defaults];
  }

  return {
    get visibleColumns(): Id[] {
      return visibleColumns;
    },
    set visibleColumns(v: Id[]) {
      visibleColumns = v;
    },
    isVisible,
    setVisible,
    resetColumns
  };
}
