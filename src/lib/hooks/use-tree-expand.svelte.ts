import { SvelteSet } from 'svelte/reactivity';

export function useTreeExpand() {
  const expanded = new SvelteSet<string>();

  function toggle(guid: string) {
    if (expanded.has(guid)) {
      expanded.delete(guid);
    } else {
      expanded.add(guid);
    }
  }

  function expandAll(guids: string[]) {
    expanded.clear();
    for (const g of guids) {
      expanded.add(g);
    }
  }

  function collapseAll() {
    expanded.clear();
  }

  return {
    expanded,
    toggle,
    expandAll,
    collapseAll
  };
}
