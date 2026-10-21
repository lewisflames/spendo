import type { AccountNode, AccountsResult } from '$lib/tauri';
import { CATEGORY_LABEL_PLURAL, nodeCategory } from './account-types';
import { subtreeTotal } from '$lib/features/reports/balance-sheet';

/** Sections of the accounts page, in chart-of-accounts order. */
const TREE_CATEGORIES = ['ASSET', 'LIABILITY', 'EQUITY', 'INCOME', 'EXPENSE'] as const;

export interface AccountTreeSection {
  category: string;
  label: string;
  /** Top-level accounts; a placeholder category root is skipped (the section names it). */
  roots: AccountNode[];
  total: number;
}

export interface AccountTreeRow {
  node: AccountNode;
  depth: number;
  hasChildren: boolean;
  expanded: boolean;
  /** Placeholder-aware subtree total, matching the balance sheet. */
  total: number;
}

export function buildAccountSections(data: AccountsResult): AccountTreeSection[] {
  return TREE_CATEGORIES.map((category) => {
    const trees = data.trees.filter((tree) => nodeCategory(tree.accountType) === category);
    return {
      category,
      label: CATEGORY_LABEL_PLURAL[category] ?? category,
      roots: trees.flatMap((tree) => (tree.placeholder ? tree.children : [tree])),
      total: trees.reduce((sum, tree) => sum + subtreeTotal(tree), 0)
    };
  }).filter((section) => section.roots.length > 0);
}

function matches(node: AccountNode, query: string): boolean {
  const q = query.toLowerCase();
  return (
    node.name.toLowerCase().includes(q) ||
    (node.code?.toLowerCase().includes(q) ?? false) ||
    (node.description?.toLowerCase().includes(q) ?? false)
  );
}

function subtreeMatches(node: AccountNode, query: string): boolean {
  return matches(node, query) || node.children.some((child) => subtreeMatches(child, query));
}

/**
 * Visible rows, depth first. With a search query, every account on the path
 * to a match is shown expanded; otherwise children follow `expanded`.
 */
export function visibleAccountRows(
  roots: AccountNode[],
  expanded: ReadonlySet<string>,
  query = '',
  depth = 0
): AccountTreeRow[] {
  const rows: AccountTreeRow[] = [];
  for (const node of roots) {
    if (query && !subtreeMatches(node, query)) continue;
    const hasChildren = node.children.length > 0;
    const isExpanded = hasChildren && (query ? true : expanded.has(node.guid));
    rows.push({ node, depth, hasChildren, expanded: isExpanded, total: subtreeTotal(node) });
    if (isExpanded) rows.push(...visibleAccountRows(node.children, expanded, query, depth + 1));
  }
  return rows;
}

/** Every account GUID with children (for "Expand all"). */
export function parentGuids(nodes: AccountNode[]): string[] {
  return nodes.flatMap((node) =>
    node.children.length > 0 ? [node.guid, ...parentGuids(node.children)] : []
  );
}
