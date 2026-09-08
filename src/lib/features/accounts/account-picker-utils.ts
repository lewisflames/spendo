import type { AccountNode } from '$lib/tauri';

export interface PickerAccount {
  guid: string;
  /** Full display path, e.g. `Expenses › Groceries`. */
  label: string;
  accountType: string;
  placeholder: boolean;
  commodity: string | null;
}

/** Flatten account trees into picker options with full display paths. */
export function flattenAccountsForPicker(
  trees: AccountNode[] | null | undefined,
  opts?: { excludeGuids?: Set<string> | string[]; hidePlaceholders?: boolean }
): PickerAccount[] {
  if (!trees) return [];
  const excluded = new Set(opts?.excludeGuids ?? []);
  const hidePlaceholders = opts?.hidePlaceholders ?? false;
  const out: PickerAccount[] = [];
  function walk(nodes: AccountNode[], ancestors: string[]) {
    for (const n of nodes) {
      const display = n.description ?? n.name;
      const path = [...ancestors, display];
      if (!excluded.has(n.guid) && !(hidePlaceholders && n.placeholder)) {
        out.push({
          guid: n.guid,
          label: path.join(' › '),
          accountType: n.accountType,
          placeholder: n.placeholder,
          commodity: n.commodity
        });
      }
      if (n.children.length > 0) walk(n.children, path);
    }
  }
  walk(trees, []);
  out.sort((a, b) => a.label.localeCompare(b.label));
  return out;
}

/** Collect a guid plus all descendant guids (for move-parent exclusion). */
export function subtreeGuids(trees: AccountNode[] | null | undefined, guid: string): Set<string> {
  const found = new Set<string>();
  function find(nodes: AccountNode[]): AccountNode | null {
    for (const n of nodes) {
      if (n.guid === guid) return n;
      const hit = find(n.children);
      if (hit) return hit;
    }
    return null;
  }
  const node = trees ? find(trees) : null;
  if (!node) return new Set([guid]);
  function collect(n: AccountNode) {
    found.add(n.guid);
    for (const c of n.children) collect(c);
  }
  collect(node);
  return found;
}
