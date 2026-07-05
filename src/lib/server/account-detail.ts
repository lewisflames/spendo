import Database from 'better-sqlite3';
import { getAppDbPath } from './app-db';
import { getCachedPricesMap } from './prices';
import { CREDIT_NORMAL } from '$lib/account-types';

export interface DetailSplit {
  guid: string;
  accountGuid: string;
  accountName: string;
  accountType: string;
  amount: number;
}

export interface DetailTransaction {
  guid: string;
  postDate: string;
  description: string;
  splits: DetailSplit[];
  runningBalance: number;
}

export interface AccountDetailInfo {
  guid: string;
  name: string;
  accountType: string;
  code: string | null;
  description: string | null;
  placeholder: boolean;
  balance: number;
  childrenCount: number;
}

export interface HierarchyItem {
  guid: string;
  name: string;
}

export interface AccountDetailResult {
  account: AccountDetailInfo;
  hierarchy: HierarchyItem[];
  summary: {
    totalTransactions: number;
    totalIncome: number;
    totalExpenses: number;
  };
  transactions: DetailTransaction[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
}

export function getAccountDetail(
  guid: string,
  page: number,
  pageSize: number,
  dateFrom?: string,
  dateTo?: string,
  search?: string
): AccountDetailResult {
  const path = getAppDbPath();
  const db = new Database(path);

  try {
    // 1. Account info
    const accountRow = db
      .prepare(
        `SELECT guid, name, account_type, parent_guid, code, description, placeholder
				 FROM accounts WHERE guid = ?`
      )
      .get(guid) as
      | {
          guid: string;
          name: string;
          account_type: string;
          parent_guid: string | null;
          code: string | null;
          description: string | null;
          placeholder: number;
        }
      | undefined;

    if (!accountRow) {
      throw new Error(`Account not found: ${guid}`);
    }

    // 2. Sub-accounts
    const subRows = db
      .prepare(
        `WITH RECURSIVE sub AS (
					SELECT guid FROM accounts WHERE guid = ?
					UNION ALL
					SELECT a.guid FROM accounts a JOIN sub ON a.parent_guid = sub.guid
				)
				SELECT guid FROM sub`
      )
      .all(guid) as { guid: string }[];

    const subGuids = subRows.map((r) => r.guid);
    const childrenCount = subGuids.length - 1;
    const placeholders = subGuids.map(() => '?').join(',');
    const subParams = [...subGuids];

    // 3. Balance (account + sub-accounts)
    // Raw value_num/value_denom sum across all sub-accounts (cost basis)
    const balanceRow = db
      .prepare(
        `SELECT COALESCE(SUM(CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL)), 0) as raw_balance
				 FROM splits s
				 JOIN transactions t ON t.guid = s.tx_guid
				 WHERE s.account_guid IN (${placeholders})
				 AND t.post_date < date('now', '+1 day')`
      )
      .get(...subParams) as { raw_balance: number };

    // Cost basis for STOCK/MUTUAL accounts only (to compute market value adjustment)
    const stockCostRow = db
      .prepare(
        `SELECT COALESCE(SUM(CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL)), 0) as cost_basis
				 FROM splits s
				 JOIN accounts a ON a.guid = s.account_guid
				 JOIN transactions t ON t.guid = s.tx_guid
				 WHERE s.account_guid IN (${placeholders})
				 AND t.post_date < date('now', '+1 day')
				 AND a.account_type IN ('STOCK', 'MUTUAL')`
      )
      .get(...subParams) as { cost_basis: number };

    // Shares × commodity for STOCK/MUTUAL (to compute market value)
    const stockQuantityRows = db
      .prepare(
        `SELECT c.mnemonic as commodity_mnemonic,
					SUM(CAST(s.quantity_num AS REAL) / CAST(s.quantity_denom AS REAL)) as quantity
				 FROM splits s
				 JOIN accounts a ON a.guid = s.account_guid
				 JOIN commodities c ON c.guid = a.commodity_guid
				 JOIN transactions t ON t.guid = s.tx_guid
				 WHERE s.account_guid IN (${placeholders})
				 AND t.post_date < date('now', '+1 day')
				 AND a.account_type IN ('STOCK', 'MUTUAL')
				 GROUP BY c.mnemonic`
      )
      .all(...subParams) as { commodity_mnemonic: string; quantity: number }[];

    // Replace cost basis with market value for STOCK/MUTUAL if cached prices exist
    let rawBalance = balanceRow.raw_balance;
    if (stockQuantityRows.length > 0) {
      const cachedPrices = getCachedPricesMap();
      let stockMarketValue = 0;
      let hasPrices = false;
      for (const row of stockQuantityRows) {
        const cp = cachedPrices?.get(row.commodity_mnemonic);
        if (cp) {
          stockMarketValue += row.quantity * cp.price;
          hasPrices = true;
        } else {
          stockMarketValue += row.quantity;
        }
      }
      if (hasPrices) {
        rawBalance = rawBalance - stockCostRow.cost_basis + stockMarketValue;
      }
    }

    const balance = CREDIT_NORMAL.has(accountRow.account_type) ? -rawBalance : rawBalance;

    // 4. Hierarchy (breadcrumb)
    const hierarchy: HierarchyItem[] = [];
    let currentGuid: string | null = guid;
    while (currentGuid) {
      const row = db
        .prepare(
          `SELECT guid, name, parent_guid
					 FROM accounts WHERE guid = ? AND account_type != 'ROOT'`
        )
        .get(currentGuid) as
        | {
            guid: string;
            name: string;
            parent_guid: string | null;
          }
        | undefined;
      if (!row) break;
      hierarchy.unshift({ guid: row.guid, name: row.name });
      currentGuid = row.parent_guid;
    }

    // 5. Summary (income = balance-increasing txns, expenses = balance-decreasing txns)
    const extraClauses: string[] = [];
    const extraParams: (string | number)[] = [];
    if (dateFrom) {
      extraClauses.push('t.post_date >= ?');
      extraParams.push(dateFrom);
    }
    if (dateTo) {
      extraClauses.push('t.post_date <= ?');
      extraParams.push(dateTo);
    }
    if (search) {
      extraClauses.push('t.description LIKE ?');
      extraParams.push(`%${search}%`);
    }
    const extraFilterSql = extraClauses.length > 0 ? 'AND ' + extraClauses.join(' AND ') : '';

    const summaryRow = db
      .prepare(
        `SELECT
					COUNT(*) as total_txns,
					COALESCE(SUM(CASE WHEN effect > 0 THEN effect ELSE 0 END), 0) as pos_effect,
					COALESCE(SUM(CASE WHEN effect < 0 THEN -effect ELSE 0 END), 0) as neg_effect
				 FROM (
					SELECT t.guid,
						SUM(CASE WHEN a.account_type IN ('INCOME','LIABILITY','EQUITY','CREDIT','PAYABLE')
							THEN -CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL)
							ELSE CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL) END) as effect
					FROM transactions t
					JOIN splits s ON s.tx_guid = t.guid
					JOIN accounts a ON a.guid = s.account_guid
					WHERE s.account_guid IN (${placeholders})
					AND t.post_date < date('now', '+1 day')
					${extraFilterSql}
					GROUP BY t.guid
				 ) sub`
      )
      .get(...subParams, ...extraParams) as {
      total_txns: number;
      pos_effect: number;
      neg_effect: number;
    };

    // 6. Total count for pagination
    const countRow = db
      .prepare(
        `SELECT COUNT(DISTINCT t.guid) as cnt
				 FROM transactions t
				 JOIN splits s ON s.tx_guid = t.guid
				 WHERE s.account_guid IN (${placeholders})
				 AND t.post_date < date('now', '+1 day')
				 ${extraFilterSql}`
      )
      .get(...subParams, ...extraParams) as { cnt: number };

    const total = countRow.cnt;
    const totalPages = Math.max(Math.ceil(total / pageSize), 1);
    const safePage = Math.min(page, totalPages);
    const offset = (safePage - 1) * pageSize;

    // 7. Paginated transactions with running balance
    const txExtraFilterSql =
      (dateFrom ? 'AND t.post_date >= ? ' : '') +
      (dateTo ? 'AND t.post_date <= ? ' : '') +
      (search ? 'AND t.description LIKE ? ' : '');

    const txExtraParams: (string | number)[] = [];
    if (dateFrom) txExtraParams.push(dateFrom);
    if (dateTo) txExtraParams.push(dateTo);
    if (search) txExtraParams.push(`%${search}%`);

    const txRows = db
      .prepare(
        `WITH RECURSIVE sub AS (
					SELECT guid FROM accounts WHERE guid = ?
					UNION ALL
					SELECT a.guid FROM accounts a JOIN sub ON a.parent_guid = sub.guid
				),
				tx_splits AS (
					SELECT t.guid, t.post_date, t.description,
						CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL) as raw_amount,
						a.account_type
					FROM transactions t
					JOIN splits s ON s.tx_guid = t.guid
					JOIN accounts a ON a.guid = s.account_guid
					WHERE s.account_guid IN (SELECT guid FROM sub)
					AND t.post_date < date('now', '+1 day')
					${txExtraFilterSql}
				)
				SELECT guid, post_date, description,
					SUM(CASE WHEN account_type IN ('INCOME','LIABILITY','EQUITY','CREDIT','PAYABLE')
						THEN -raw_amount ELSE raw_amount END) as balance_effect,
					SUM(SUM(CASE WHEN account_type IN ('INCOME','LIABILITY','EQUITY','CREDIT','PAYABLE')
						THEN -raw_amount ELSE raw_amount END)) OVER (
						ORDER BY post_date ASC, guid ASC
					) as running_balance
				FROM tx_splits
				GROUP BY guid, post_date, description
				ORDER BY post_date DESC, guid DESC
				LIMIT ? OFFSET ?`
      )
      .all(guid, ...txExtraParams, pageSize, offset) as {
      guid: string;
      post_date: string;
      description: string;
      balance_effect: number;
      running_balance: number;
    }[];

    // 8. Split details for each transaction
    const splitStmt = db.prepare(
      `SELECT s.guid, s.account_guid, a.name as account_name, a.account_type,
				CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL) as amount
			 FROM splits s
			 JOIN accounts a ON a.guid = s.account_guid
			 WHERE s.tx_guid = ?
			 ORDER BY ABS(s.value_num) DESC`
    );

    const transactions: DetailTransaction[] = [];
    for (const row of txRows) {
      const splits = splitStmt.all(row.guid) as {
        guid: string;
        account_guid: string;
        account_name: string;
        account_type: string;
        amount: number;
      }[];

      transactions.push({
        guid: row.guid,
        postDate: row.post_date,
        description: row.description,
        splits: splits.map((s) => ({
          guid: s.guid,
          accountGuid: s.account_guid,
          accountName: s.account_name,
          accountType: s.account_type,
          amount: s.amount
        })),
        runningBalance: row.running_balance
      });
    }

    return {
      account: {
        guid: accountRow.guid,
        name: accountRow.name,
        accountType: accountRow.account_type,
        code: accountRow.code,
        description: accountRow.description,
        placeholder: accountRow.placeholder === 1,
        balance,
        childrenCount
      },
      hierarchy,
      summary: {
        totalTransactions: summaryRow.total_txns,
        totalIncome: summaryRow.pos_effect,
        totalExpenses: summaryRow.neg_effect
      },
      transactions,
      total,
      page: safePage,
      pageSize,
      totalPages
    };
  } finally {
    db.close();
  }
}
