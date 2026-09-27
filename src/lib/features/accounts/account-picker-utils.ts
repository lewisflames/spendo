import type { AccountNode } from '$lib/tauri';

export interface PickerAccount {
  guid: string;
  /** Full display path, e.g. `Expenses › Groceries` (search, sort, tooltips). */
  label: string;
  /** Leaf account name, e.g. `Groceries`. */
  display: string;
  /** Optional note shown under the name; never used as the display label. */
  description: string | null;
  /** Optional account code, shown as a chip next to the name. */
  code: string | null;
  /** Ancestor display path; set only when `display` is shared by several options. */
  parentPath: string | null;
  accountType: string;
  placeholder: boolean;
  commodity: string | null;
}

/** Null out `parentPath` entries whose `display` is unique among `rows`. */
export function keepDuplicateParentPaths<T extends { display: string; parentPath: string | null }>(
  rows: T[]
): void {
  const counts = new Map<string, number>();
  for (const row of rows) counts.set(row.display, (counts.get(row.display) ?? 0) + 1);
  for (const row of rows) {
    if ((counts.get(row.display) ?? 0) < 2) row.parentPath = null;
  }
}

/** Flatten account trees into picker options with leaf display names. */
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
      const display = n.name;
      const path = [...ancestors, display];
      if (!excluded.has(n.guid) && !(hidePlaceholders && n.placeholder)) {
        out.push({
          guid: n.guid,
          label: path.join(' › '),
          display,
          description: n.description,
          code: n.code,
          parentPath: path.slice(0, -1).join(' › ') || null,
          accountType: n.accountType,
          placeholder: n.placeholder,
          commodity: n.commodity
        });
      }
      if (n.children.length > 0) walk(n.children, path);
    }
  }
  walk(trees, []);
  keepDuplicateParentPaths(out);
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
