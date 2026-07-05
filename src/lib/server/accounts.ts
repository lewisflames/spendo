import Database from 'better-sqlite3';
import { getAppDbPath } from './app-db';
import { getUnifiedPricesMap } from './prices';
import { CREDIT_NORMAL, TYPE_CATEGORY } from '$lib/account-types';

interface AccountRow {
  guid: string;
  name: string;
  account_type: string;
  parent_guid: string | null;
  code: string | null;
  description: string | null;
  placeholder: number;
  balance: number;
  commodity_mnemonic: string | null;
  quantity: number;
}

export interface AccountNode {
  guid: string;
  name: string;
  accountType: string;
  code: string | null;
  description: string | null;
  placeholder: boolean;
  balance: number;
  children: AccountNode[];
}

export interface AccountsResult {
  trees: AccountNode[];
  byType: Record<string, { count: number; totalBalance: number }>;
  totalAccounts: number;
}

export function getAccounts(): AccountsResult {
  const path = getAppDbPath();
  const db = new Database(path);

  try {
    const rows = db
      .prepare(
        `SELECT a.guid, a.name, a.account_type, a.parent_guid, a.code, a.description, a.placeholder,
				        c.mnemonic as commodity_mnemonic,
				        COALESCE(b.balance, 0) as balance,
				        COALESCE(q.quantity, 0) as quantity
				 FROM accounts a
				 LEFT JOIN commodities c ON c.guid = a.commodity_guid
				 LEFT JOIN (
				   SELECT s.account_guid,
				          SUM(CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL)) as balance
				   FROM splits s
				   JOIN transactions t ON t.guid = s.tx_guid
				   WHERE t.post_date < date('now', '+1 day')
				   GROUP BY s.account_guid
				 ) b ON b.account_guid = a.guid
				 LEFT JOIN (
				   SELECT s.account_guid,
				          SUM(CAST(s.quantity_num AS REAL) / CAST(s.quantity_denom AS REAL)) as quantity
				   FROM splits s
				   JOIN transactions t ON t.guid = s.tx_guid
				   WHERE t.post_date < date('now', '+1 day')
				   GROUP BY s.account_guid
				 ) q ON q.account_guid = a.guid
				 WHERE a.account_type != 'ROOT'
				 ORDER BY a.account_type, a.name`
      )
      .all() as AccountRow[];

    const nodeMap = new Map<string, AccountNode>();
    const rootNodes: AccountNode[] = [];
    const byType: Record<string, { count: number; totalBalance: number }> = {};

    const unifiedPrices = getUnifiedPricesMap();

    for (const row of rows) {
      let balance = CREDIT_NORMAL.has(row.account_type) ? -row.balance : row.balance;

      if (
        (row.account_type === 'STOCK' || row.account_type === 'MUTUAL') &&
        row.commodity_mnemonic
      ) {
        const price = unifiedPrices?.get(row.commodity_mnemonic);
        if (price) {
          const adjustedBalance = CREDIT_NORMAL.has(row.account_type)
            ? -row.quantity
            : row.quantity;
          balance = adjustedBalance * price;
        }
      }

      const node: AccountNode = {
        guid: row.guid,
        name: row.name,
        accountType: row.account_type,
        code: row.code,
        description: row.description,
        placeholder: row.placeholder === 1,
        balance,
        children: []
      };
      nodeMap.set(row.guid, node);

      const category = TYPE_CATEGORY[row.account_type] ?? row.account_type;
      if (!byType[category]) {
        byType[category] = { count: 0, totalBalance: 0 };
      }
      byType[category].count++;
      byType[category].totalBalance += balance;
    }

    for (const row of rows) {
      const node = nodeMap.get(row.guid);
      if (!node) continue;
      if (row.parent_guid) {
        const parent = nodeMap.get(row.parent_guid);
        if (parent) {
          parent.children.push(node);
        } else {
          rootNodes.push(node);
        }
      } else {
        rootNodes.push(node);
      }
    }

    function sortChildren(nodes: AccountNode[]) {
      nodes.sort((a, b) => a.name.localeCompare(b.name));
      for (const n of nodes) sortChildren(n.children);
    }
    sortChildren(rootNodes);

    function propagateBalance(nodes: AccountNode[]) {
      for (const node of nodes) {
        propagateBalance(node.children);
        for (const child of node.children) {
          node.balance += child.balance;
        }
      }
    }
    propagateBalance(rootNodes);

    return { trees: rootNodes, byType, totalAccounts: rows.length };
  } finally {
    db.close();
  }
}
