import type { ProfitLossRow } from '$lib/tauri';
import type { PnLTreeNode } from './pnl-types';

/** Rows arrive parent-before-child with depth-first indents; a single
 * forward pass with an ancestry stack rebuilds the tree in order. */
export function buildTree(rows: ProfitLossRow[]): PnLTreeNode[] {
  const root: PnLTreeNode[] = [];
  const stack: { indent: number; node: PnLTreeNode }[] = [];

  for (const row of rows) {
    const node: PnLTreeNode = { row, children: [] };
    while (stack.length > 0 && stack[stack.length - 1].indent >= row.indent) {
      stack.pop();
    }
    if (stack.length === 0) {
      root.push(node);
    } else {
      stack[stack.length - 1].node.children.push(node);
    }
    stack.push({ indent: row.indent, node });
  }

  return root;
}

export function flattenTree(
  nodes: PnLTreeNode[],
  expanded: Set<string>,
  depth = 0
): { node: PnLTreeNode; depth: number }[] {
  const result: { node: PnLTreeNode; depth: number }[] = [];
  for (const n of nodes) {
    result.push({ node: n, depth });
    if (n.children.length > 0 && expanded.has(n.row.accountGuid)) {
      result.push(...flattenTree(n.children, expanded, depth + 1));
    }
  }
  return result;
}

export function collectParentGuids(nodes: PnLTreeNode[]): string[] {
  const guids: string[] = [];
  for (const n of nodes) {
    if (n.children.length > 0) {
      guids.push(n.row.accountGuid);
      guids.push(...collectParentGuids(n.children));
    }
  }
  return guids;
}

/** Change pct with backend-compatible divide-by-zero: null when both zero,
 * otherwise relative change (previous==0, current!=0 yields 0 to match the
 * frontend convention; the backend reports 100 — see reports.rs). */
export function changePct(current: number, previous: number): number | null {
  if (previous !== 0) return ((current - previous) / previous) * 100;
  if (current !== 0) return 0;
  return null;
}

/** Text color for a variance amount: green when "good", red otherwise. */
export function varianceColor(value: number, positiveGood: boolean): string {
  if (positiveGood) return value >= 0 ? 'text-success' : 'text-destructive';
  return value >= 0 ? 'text-destructive' : 'text-success';
}
