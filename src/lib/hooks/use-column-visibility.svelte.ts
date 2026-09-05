/**
 * Column visibility state shared by tables with a columns menu
 * (account ledger, holdings). Preserves `COLUMNS` order on show.
 */
export function useColumnVisibility<Id extends string>(defaults: Id[]) {
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
