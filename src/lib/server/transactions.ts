import Database from 'better-sqlite3';
import { getDbPath } from './uploaded-db';

export interface SplitInfo {
  guid: string;
  accountGuid: string;
  accountName: string;
  accountType: string;
  amount: number;
}

export interface TransactionRow {
  guid: string;
  postDate: string;
  description: string;
  splits: SplitInfo[];
}

export interface AccountInfo {
  name: string;
}

export interface Filters {
  search?: string;
  dateFrom?: string;
  dateTo?: string;
  accountName?: string;
}

interface SplitRow {
  guid: string;
  tx_guid: string;
  account_guid: string;
  account_name: string;
  account_type: string;
  amount: number;
}

export interface TransactionsResult {
  transactions: TransactionRow[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
}

export function getAccounts(token: string): AccountInfo[] {
  const path = getDbPath(token);
  const db = new Database(path, { readonly: true });

  try {
    return db
      .prepare(
        `SELECT DISTINCT a.name
				 FROM accounts a
				 JOIN splits s ON s.account_guid = a.guid
				 ORDER BY a.name`
      )
      .all() as AccountInfo[];
  } finally {
    db.close();
  }
}

function buildFilterConditions(filters?: Filters): {
  conditions: string[];
  params: unknown[];
} {
  const conditions: string[] = ["t.post_date < date('now', '+1 day')"];
  const params: unknown[] = [];

  if (filters?.search) {
    conditions.push("t.description LIKE '%' || ? || '%'");
    params.push(filters.search);
  }

  if (filters?.dateFrom) {
    conditions.push('t.post_date >= ?');
    params.push(filters.dateFrom);
  }

  if (filters?.dateTo) {
    conditions.push('t.post_date <= ?');
    params.push(filters.dateTo);
  }

  if (filters?.accountName) {
    conditions.push(
      't.guid IN (SELECT s.tx_guid FROM splits s JOIN accounts a ON a.guid = s.account_guid WHERE a.name = ?)'
    );
    params.push(filters.accountName);
  }

  return { conditions, params };
}

function fetchSplits(db: Database.Database, guids: string[]): Map<string, SplitRow[]> {
  if (guids.length === 0) return new Map();
  const placeholders = guids.map(() => '?').join(',');
  const rows = db
    .prepare(
      `SELECT s.guid, s.tx_guid, s.account_guid, a.name as account_name, a.account_type,
					CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL) as amount
			 FROM splits s
			 JOIN accounts a ON a.guid = s.account_guid
			 WHERE s.tx_guid IN (${placeholders})
			 ORDER BY s.tx_guid, ABS(s.value_num) DESC`
    )
    .all(...guids) as SplitRow[];

  const map = new Map<string, SplitRow[]>();
  for (const row of rows) {
    const list = map.get(row.tx_guid);
    if (list) {
      list.push(row);
    } else {
      map.set(row.tx_guid, [row]);
    }
  }
  return map;
}

function mapTransaction(
  row: { guid: string; post_date: string; description: string },
  splits: SplitRow[]
): TransactionRow {
  return {
    guid: row.guid,
    postDate: row.post_date,
    description: row.description,
    splits: splits.map((s) => ({
      guid: s.guid,
      accountGuid: s.account_guid,
      accountName: s.account_name,
      accountType: s.account_type,
      amount: s.amount
    }))
  };
}

export function getAllTransactionsFiltered(token: string, filters?: Filters): TransactionRow[] {
  const path = getDbPath(token);
  const db = new Database(path, { readonly: true });

  try {
    const { conditions, params } = buildFilterConditions(filters);
    const where = conditions.length > 0 ? ` WHERE ${conditions.join(' AND ')}` : '';

    const rows = db
      .prepare(
        `SELECT guid, post_date, description FROM transactions t${where} ORDER BY post_date DESC`
      )
      .all(...params) as {
      guid: string;
      post_date: string;
      description: string;
    }[];

    const guids = rows.map((r) => r.guid);
    const splitMap = fetchSplits(db, guids);

    return rows.map((row) => mapTransaction(row, splitMap.get(row.guid) ?? []));
  } finally {
    db.close();
  }
}

export function getTransactions(
  token: string,
  page: number,
  pageSize: number,
  filters?: Filters
): TransactionsResult {
  const path = getDbPath(token);
  const db = new Database(path, { readonly: true });

  try {
    const { conditions, params } = buildFilterConditions(filters);
    const where = conditions.length > 0 ? ` WHERE ${conditions.join(' AND ')}` : '';

    const countRow = db
      .prepare(`SELECT COUNT(*) as cnt FROM transactions t${where}`)
      .get(...params) as { cnt: number } | undefined;
    const total = countRow?.cnt ?? 0;
    const totalPages = Math.max(Math.ceil(total / pageSize), 1);
    const offset = (page - 1) * pageSize;

    const rows = db
      .prepare(
        `SELECT guid, post_date, description FROM transactions t${where} ORDER BY post_date DESC LIMIT ? OFFSET ?`
      )
      .all(...params, pageSize, offset) as {
      guid: string;
      post_date: string;
      description: string;
    }[];

    const guids = rows.map((r) => r.guid);
    const splitMap = fetchSplits(db, guids);

    const transactions = rows.map((row) => mapTransaction(row, splitMap.get(row.guid) ?? []));

    return { transactions, total, page, pageSize, totalPages };
  } finally {
    db.close();
  }
}
