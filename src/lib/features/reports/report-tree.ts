/** Minimal shape for indent-based report rows (P&L, balance sheet). */
export interface IndentedRow {
  accountGuid: string;
  indent: number;
}

export interface ReportTreeNode<T extends IndentedRow> {
  row: T;
  children: ReportTreeNode<T>[];
}

/** Rows arrive parent-before-child with depth-first indents; a single
 * forward pass with an ancestry stack rebuilds the tree in order. */
export function buildTree<T extends IndentedRow>(rows: T[]): ReportTreeNode<T>[] {
  const root: ReportTreeNode<T>[] = [];
  const stack: { indent: number; node: ReportTreeNode<T> }[] = [];

  for (const row of rows) {
    const node: ReportTreeNode<T> = { row, children: [] };
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

export function flattenTree<T extends IndentedRow>(
  nodes: ReportTreeNode<T>[],
  expanded: Set<string>,
  depth = 0
): { node: ReportTreeNode<T>; depth: number }[] {
  const result: { node: ReportTreeNode<T>; depth: number }[] = [];
  for (const n of nodes) {
    result.push({ node: n, depth });
    if (n.children.length > 0 && expanded.has(n.row.accountGuid)) {
      result.push(...flattenTree(n.children, expanded, depth + 1));
    }
  }
  return result;
}

export function collectParentGuids<T extends IndentedRow>(nodes: ReportTreeNode<T>[]): string[] {
  const guids: string[] = [];
  for (const n of nodes) {
    if (n.children.length > 0) {
      guids.push(n.row.accountGuid);
      guids.push(...collectParentGuids(n.children));
    }
  }
  return guids;
}

/** Flatten ignoring UI collapse state (every parent treated as expanded), for
 * full-data exports that must not depend on which rows the user collapsed. */
export function flattenAll<T extends IndentedRow>(
  nodes: ReportTreeNode<T>[]
): { node: ReportTreeNode<T>; depth: number }[] {
  return flattenTree(nodes, new Set(collectParentGuids(nodes)));
}
