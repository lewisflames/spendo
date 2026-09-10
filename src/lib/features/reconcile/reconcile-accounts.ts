import type { AccountNode } from '$lib/tauri';
import { keepDuplicateParentPaths } from '$lib/features/accounts/account-picker-utils';
import { isReconcilable } from './math';

export interface ReconcilableAccount {
  guid: string;
  /** Full display path, e.g. `Assets › Checking`. */
  label: string;
  /** Leaf display name, e.g. `Checking`. */
  display: string;
  /** Ancestor display path; set only when `display` is shared by several rows. */
  parentPath: string | null;
  name: string;
  description: string | null;
  code: string | null;
  commodity: string | null;
  accountType: string;
  balance: number;
  txCount: number;
}

/** Flatten account trees to reconcilable accounts (cash-like, non-placeholder)
 * with leaf display names, sorted by full path. */
export function flattenReconcilableAccounts(
  trees: AccountNode[] | null | undefined
): ReconcilableAccount[] {
  if (!trees) return [];
  const out: ReconcilableAccount[] = [];
  function walk(nodes: AccountNode[], ancestors: string[]) {
    for (const n of nodes) {
      const display = n.description ?? n.name;
      const path = [...ancestors, display];
      if (isReconcilable(n.accountType, n.placeholder)) {
        out.push({
          guid: n.guid,
          label: path.join(' › '),
          display,
          parentPath: path.slice(0, -1).join(' › ') || null,
          name: n.name,
          description: n.description,
          code: n.code,
          commodity: n.commodity,
          accountType: n.accountType,
          balance: n.balance,
          txCount: n.txCount
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
