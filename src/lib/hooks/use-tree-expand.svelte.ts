import { SvelteSet } from 'svelte/reactivity';

export function useTreeExpand<K = string>() {
  const expanded = new SvelteSet<K>();

  function toggle(guid: K) {
    if (expanded.has(guid)) {
      expanded.delete(guid);
    } else {
      expanded.add(guid);
    }
  }

  function expandAll(guids: K[]) {
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
