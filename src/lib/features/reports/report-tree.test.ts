import { describe, expect, it } from 'vitest';
import {
  buildTree,
  collectParentGuids,
  flattenAll,
  flattenTree,
  type ReportTreeNode
} from './report-tree';

interface Row {
  accountGuid: string;
  indent: number;
}

function row(accountGuid: string, indent: number): Row {
  return { accountGuid, indent };
}

function childGuids(node: ReportTreeNode<Row>): string[] {
  return node.children.map((c) => c.row.accountGuid);
}

describe('buildTree', () => {
  it('returns empty for no rows', () => {
    expect(buildTree([])).toEqual([]);
  });

  it('keeps flat rows as roots', () => {
    const tree = buildTree([row('a', 0), row('b', 0)]);
    expect(tree.map((n) => n.row.accountGuid)).toEqual(['a', 'b']);
    expect(tree.every((n) => n.children.length === 0)).toBe(true);
  });

  it('nests children under the preceding parent', () => {
    const tree = buildTree([row('a', 0), row('b', 1), row('c', 1), row('d', 0)]);
    expect(tree.map((n) => n.row.accountGuid)).toEqual(['a', 'd']);
    expect(childGuids(tree[0])).toEqual(['b', 'c']);
  });

  it('nests grandchildren two levels deep', () => {
    const tree = buildTree([row('a', 0), row('b', 1), row('c', 2)]);
    expect(childGuids(tree[0])).toEqual(['b']);
    expect(childGuids(tree[0].children[0])).toEqual(['c']);
  });

  it('pops back to the correct ancestor on dedent', () => {
    const tree = buildTree([row('a', 0), row('b', 1), row('c', 2), row('d', 1)]);
    expect(childGuids(tree[0])).toEqual(['b', 'd']);
    expect(childGuids(tree[0].children[0])).toEqual(['c']);
  });
});

describe('flattenTree', () => {
  const tree = buildTree([row('a', 0), row('b', 1), row('c', 2), row('d', 0)]);

  it('shows only roots when nothing is expanded', () => {
    const flat = flattenTree(tree, new Set());
    expect(flat.map((f) => f.node.row.accountGuid)).toEqual(['a', 'd']);
    expect(flat.map((f) => f.depth)).toEqual([0, 0]);
  });

  it('expands one level with depths', () => {
    const flat = flattenTree(tree, new Set(['a']));
    expect(flat.map((f) => f.node.row.accountGuid)).toEqual(['a', 'b', 'd']);
    expect(flat.map((f) => f.depth)).toEqual([0, 1, 0]);
  });

  it('expands recursively', () => {
    const flat = flattenTree(tree, new Set(['a', 'b']));
    expect(flat.map((f) => f.node.row.accountGuid)).toEqual(['a', 'b', 'c', 'd']);
    expect(flat.map((f) => f.depth)).toEqual([0, 1, 2, 0]);
  });
});

describe('collectParentGuids / flattenAll', () => {
  const tree = buildTree([row('a', 0), row('b', 1), row('c', 0)]);

  it('collects only guids that have children', () => {
    expect(collectParentGuids(tree)).toEqual(['a']);
  });

  it('flattenAll ignores collapse state', () => {
    const flat = flattenAll(tree);
    expect(flat.map((f) => f.node.row.accountGuid)).toEqual(['a', 'b', 'c']);
  });
});
