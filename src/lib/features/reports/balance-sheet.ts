import type { AccountNode, AccountsResult } from '$lib/tauri';
import { CATEGORY_LABEL_PLURAL, nodeCategory } from '$lib/features/accounts/account-types';

/** Categories a balance sheet reports, in statement order. */
export type BalanceSheetCategory = 'ASSET' | 'LIABILITY' | 'EQUITY';

/** Amounts below this read as zero, matching the balanced check. */
const EPSILON = 0.005;

export interface BalanceSheetRow {
  /** Empty for synthetic lines (retained earnings, unrealized gains). */
  accountGuid: string;
  name: string;
  description: string | null;
  /** Optional account code, shown as a chip next to the name. */
  code: string | null;
  /** Depth-first indent level within its section. */
  depth: number;
  /** Placeholder-aware subtree total in the report currency. */
  amount: number;
  /** The same line on the comparison date; `null` without a comparison. */
  compareAmount: number | null;
}

export interface BalanceSheetSection {
  category: BalanceSheetCategory;
  label: string;
  rows: BalanceSheetRow[];
  total: number;
  compareTotal: number | null;
}

export interface BalanceSheet {
  assets: BalanceSheetSection;
  liabilities: BalanceSheetSection;
  equity: BalanceSheetSection;
  /** Total liabilities plus total equity (includes retained + unrealized). */
  totalLiabilitiesAndEquity: number;
  compareTotalLiabilitiesAndEquity: number | null;
}

/**
 * Display total for one account node. The backend only rolls children into
 * placeholder parents (`propagate_placeholder`), so non-placeholders add their
 * subtree here; summing placeholders again would double count. Mirrors the
 * rolled-up P&L report rows.
 */
export function subtreeTotal(node: AccountNode): number {
  if (node.placeholder) return node.balance;
  return node.balance + node.children.reduce((sum, child) => sum + subtreeTotal(child), 0);
}

/** Category roots of one balance-sheet category, in backend order. */
function categoryTrees(trees: AccountNode[], category: BalanceSheetCategory): AccountNode[] {
  return trees.filter((tree) => nodeCategory(tree.accountType) === category);
}

function flattenRows(nodes: AccountNode[], depth = 0): BalanceSheetRow[] {
  return nodes.flatMap((node) => {
    // A top-level placeholder is the category group node; the section header
    // already names it, and the section total comes from the backend's
    // per-category sum. Non-placeholder roots can hold money, so they stay.
    const ownRow =
      depth === 0 && node.placeholder
        ? []
        : [
            {
              accountGuid: node.guid,
              name: node.name,
              description: node.description,
              code: node.code,
              depth,
              amount: subtreeTotal(node),
              compareAmount: null
            }
          ];
    return [...ownRow, ...flattenRows(node.children, depth + 1)];
  });
}

function accountSection(
  data: AccountsResult,
  category: BalanceSheetCategory,
  extraRows: BalanceSheetRow[] = [],
  extraTotal = 0
): BalanceSheetSection {
  return {
    category,
    label: CATEGORY_LABEL_PLURAL[category],
    rows: [...flattenRows(categoryTrees(data.trees, category)), ...extraRows],
    total: (data.byType[category]?.totalBalance ?? 0) + extraTotal,
    compareTotal: null
  };
}

/**
 * Retained earnings and unrealized gains are equity components because the
 * backend marks investments to market while equity accounts keep cost; the
 * balance identity (`balanceCheck`) only holds when both are part of equity.
 */
function buildStatement(data: AccountsResult): BalanceSheet {
  const synthetic = (name: string, amount: number): BalanceSheetRow => ({
    accountGuid: '',
    name,
    description: null,
    code: null,
    depth: 0,
    amount,
    compareAmount: null
  });

  const equityExtras: BalanceSheetRow[] = [synthetic('Retained earnings', data.retainedEarnings)];
  let equityExtraTotal = data.retainedEarnings;
  if (Math.abs(data.unrealizedGains) >= EPSILON) {
    equityExtras.push(synthetic('Unrealized gains', data.unrealizedGains));
  }
  equityExtraTotal += data.unrealizedGains;

  const assets = accountSection(data, 'ASSET');
  const liabilities = accountSection(data, 'LIABILITY');
  const equity = accountSection(data, 'EQUITY', equityExtras, equityExtraTotal);

  return {
    assets,
    liabilities,
    equity,
    totalLiabilitiesAndEquity: liabilities.total + equity.total,
    compareTotalLiabilitiesAndEquity: null
  };
}

/** Accounts match by GUID, synthetic lines (no GUID) by name. */
const rowKey = (row: BalanceSheetRow) => row.accountGuid || row.name;

/**
 * Pair each line with its comparison amount. Both dates load the same account
 * tree, so only synthetic lines can differ (unrealized gains appear only when
 * non-zero); one present on a single date shows zero on the other.
 */
function withCompare(
  current: BalanceSheetSection,
  compare: BalanceSheetSection
): BalanceSheetSection {
  const compareByKey = new Map(compare.rows.map((row) => [rowKey(row), row.amount]));
  const currentKeys = new Set(current.rows.map(rowKey));
  const onlyCompared = compare.rows
    .filter((row) => !currentKeys.has(rowKey(row)))
    .map((row) => ({ ...row, amount: 0, compareAmount: row.amount }));
  return {
    ...current,
    rows: [
      ...current.rows.map((row) => ({ ...row, compareAmount: compareByKey.get(rowKey(row)) ?? 0 })),
      ...onlyCompared
    ],
    compareTotal: compare.total
  };
}

/**
 * Build the statement from the accounts payload, optionally with a second
 * payload for another date that fills every `compare*` field.
 */
export function buildBalanceSheet(
  data: AccountsResult,
  compareData: AccountsResult | null = null
): BalanceSheet {
  const current = buildStatement(data);
  if (!compareData) return current;
  const compare = buildStatement(compareData);
  return {
    assets: withCompare(current.assets, compare.assets),
    liabilities: withCompare(current.liabilities, compare.liabilities),
    equity: withCompare(current.equity, compare.equity),
    totalLiabilitiesAndEquity: current.totalLiabilitiesAndEquity,
    compareTotalLiabilitiesAndEquity: compare.totalLiabilitiesAndEquity
  };
}

/** Comparison dates the balance sheet offers, relative to its as-of date. */
export type BalanceSheetComparison = 'month' | 'year' | 'yoy';

export const COMPARISON_OPTIONS: { value: BalanceSheetComparison; label: string }[] = [
  { value: 'month', label: 'End of previous month' },
  { value: 'year', label: 'End of previous year' },
  { value: 'yoy', label: 'Same day last year' }
];

export function isComparison(value: string | null): value is BalanceSheetComparison {
  return COMPARISON_OPTIONS.some((option) => option.value === value);
}

/** `YYYY-MM-DD` of a UTC date; Date.UTC day 0 is the previous month's last day. */
const isoUtc = (y: number, m: number, d: number) =>
  new Date(Date.UTC(y, m, d)).toISOString().slice(0, 10);

/** The comparison date for `asOf`; 29 February a year back becomes the 28th. */
export function comparisonDate(asOf: string, kind: BalanceSheetComparison): string {
  const [y, m, d] = asOf.split('-').map(Number);
  switch (kind) {
    case 'month':
      return isoUtc(y, m - 1, 0);
    case 'year':
      return `${y - 1}-12-31`;
    case 'yoy':
      return isoUtc(y - 1, m - 1, Math.min(d, new Date(Date.UTC(y - 1, m, 0)).getUTCDate()));
  }
}
