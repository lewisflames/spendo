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
}

export interface BalanceSheetSection {
  category: BalanceSheetCategory;
  label: string;
  rows: BalanceSheetRow[];
  total: number;
}

export interface BalanceSheet {
  assets: BalanceSheetSection;
  liabilities: BalanceSheetSection;
  equity: BalanceSheetSection;
  /** Total liabilities plus total equity (includes retained + unrealized). */
  totalLiabilitiesAndEquity: number;
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
              amount: subtreeTotal(node)
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
    total: (data.byType[category]?.totalBalance ?? 0) + extraTotal
  };
}

/**
 * Build the two-column statement from the accounts payload. Retained earnings
 * and unrealized gains are equity components because the backend marks
 * investments to market while equity accounts keep cost; the balance identity
 * (`balanceCheck`) only holds when both are part of equity.
 */
export function buildBalanceSheet(data: AccountsResult): BalanceSheet {
  const synthetic = (name: string, amount: number): BalanceSheetRow => ({
    accountGuid: '',
    name,
    description: null,
    code: null,
    depth: 0,
    amount
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
    totalLiabilitiesAndEquity: liabilities.total + equity.total
  };
}
