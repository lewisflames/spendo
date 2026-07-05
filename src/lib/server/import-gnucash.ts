import Database from 'better-sqlite3';
import { getAppDb } from './app-db';
import { parseGnucashDb } from './gnucash-parser';
import type { GnucashSummary } from './gnucash-parser';

export interface ImportLogEntry {
  filename: string;
  importedAt: string;
}

export function getImportLog(): ImportLogEntry | null {
  const db = getAppDb();
  try {
    const row = db
      .prepare('SELECT filename, imported_at FROM import_log ORDER BY id DESC LIMIT 1')
      .get() as { filename: string; imported_at: string } | undefined;
    if (!row) return null;
    return { filename: row.filename, importedAt: row.imported_at };
  } finally {
    db.close();
  }
}

export interface ImportLogDetails extends ImportLogEntry {
  id: number;
  accountsAdded: number;
  transactionsAdded: number;
  splitsAdded: number;
  commoditiesAdded: number;
  accountsRemoved: number;
  transactionsRemoved: number;
  splitsRemoved: number;
}

export function getImportLogs(limit = 10): ImportLogDetails[] {
  const db = getAppDb();
  try {
    const rows = db
      .prepare(
        'SELECT id, filename, imported_at, accounts_added, transactions_added, splits_added, commodities_added, accounts_removed, transactions_removed, splits_removed FROM import_log ORDER BY id DESC LIMIT ?'
      )
      .all(limit) as {
      id: number;
      filename: string;
      imported_at: string;
      accounts_added: number;
      transactions_added: number;
      splits_added: number;
      commodities_added: number;
      accounts_removed: number;
      transactions_removed: number;
      splits_removed: number;
    }[];
    return rows.map((r) => ({
      id: r.id,
      filename: r.filename,
      importedAt: r.imported_at,
      accountsAdded: r.accounts_added,
      transactionsAdded: r.transactions_added,
      splitsAdded: r.splits_added,
      commoditiesAdded: r.commodities_added,
      accountsRemoved: r.accounts_removed,
      transactionsRemoved: r.transactions_removed,
      splitsRemoved: r.splits_removed
    }));
  } finally {
    db.close();
  }
}

export function deleteImportLog(id: number) {
  const db = getAppDb();
  try {
    db.prepare('DELETE FROM import_log WHERE id = ?').run(id);
  } finally {
    db.close();
  }
}

export interface ImportResult {
  accountsAdded: number;
  transactionsAdded: number;
  splitsAdded: number;
  commoditiesAdded: number;
  totalAccounts: number;
  totalTransactions: number;
  summary: GnucashSummary;
}

const ALLOWED_REMOVAL_TABLES: Record<string, true> = {
  accounts: true,
  transactions: true,
  splits: true
};

function computeRemoved(dest: Database.Database, table: string, sourceGuids: string[]): number {
  if (sourceGuids.length === 0) return 0;
  if (!ALLOWED_REMOVAL_TABLES[table]) {
    throw new Error(`Invalid table name for removal check: ${table}`);
  }
  const placeholders = sourceGuids.map(() => '?').join(',');
  const existing = (
    dest
      .prepare(`SELECT COUNT(*) as cnt FROM ${table} WHERE guid IN (${placeholders})`)
      .get(...sourceGuids) as { cnt: number }
  ).cnt;
  const totalInDb = (dest.prepare(`SELECT COUNT(*) as cnt FROM ${table}`).get() as { cnt: number })
    .cnt;
  return totalInDb - existing;
}

export function importGnucash(uploadedPath: string, filename: string): ImportResult {
  const source = new Database(uploadedPath, { readonly: true });
  const dest = getAppDb();

  try {
    // ---- 1. Read all source data (outside transaction) ----
    const srcCommodities = source
      .prepare('SELECT guid, mnemonic, namespace, fullname FROM commodities')
      .all() as { guid: string; mnemonic: string; namespace: string; fullname: string | null }[];

    const srcAccounts = source
      .prepare(
        'SELECT guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid FROM accounts'
      )
      .all() as {
      guid: string;
      name: string;
      account_type: string;
      parent_guid: string | null;
      code: string | null;
      description: string | null;
      placeholder: number;
      commodity_guid: string | null;
    }[];

    const srcTxns = source
      .prepare('SELECT guid, post_date, description FROM transactions')
      .all() as { guid: string; post_date: string; description: string }[];

    const srcSplits = source
      .prepare(
        'SELECT guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom FROM splits'
      )
      .all() as {
      guid: string;
      tx_guid: string;
      account_guid: string;
      value_num: number;
      value_denom: number;
      quantity_num: number;
      quantity_denom: number;
    }[];

    const srcPrices = source
      .prepare(
        'SELECT guid, commodity_guid, currency_guid, value_num, value_denom, date FROM prices'
      )
      .all() as {
      guid: string;
      commodity_guid: string;
      currency_guid: string;
      value_num: number;
      value_denom: number;
      date: string;
    }[];

    // ---- 2. Compute removals (reads against app DB, outside transaction) ----
    const accountsRemoved = computeRemoved(
      dest,
      'accounts',
      srcAccounts.map((r) => r.guid)
    );
    const transactionsRemoved = computeRemoved(
      dest,
      'transactions',
      srcTxns.map((r) => r.guid)
    );
    const splitsRemoved = computeRemoved(
      dest,
      'splits',
      srcSplits.map((r) => r.guid)
    );

    // ---- 3. Prepare insert statements ----
    const insertCommodity = dest.prepare(
      'INSERT OR IGNORE INTO commodities (guid, mnemonic, namespace, fullname) VALUES (?, ?, ?, ?)'
    );
    const insertAccount = dest.prepare(
      'INSERT OR IGNORE INTO accounts (guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid) VALUES (?, ?, ?, ?, ?, ?, ?, ?)'
    );
    const insertTx = dest.prepare(
      'INSERT OR IGNORE INTO transactions (guid, post_date, description) VALUES (?, ?, ?)'
    );
    const insertSplit = dest.prepare(
      'INSERT OR IGNORE INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom) VALUES (?, ?, ?, ?, ?, ?, ?)'
    );
    const insertPrice = dest.prepare(
      'INSERT OR IGNORE INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date) VALUES (?, ?, ?, ?, ?, ?)'
    );
    const insertLog = dest.prepare(
      'INSERT INTO import_log (filename, imported_at, accounts_added, transactions_added, splits_added, commodities_added, accounts_removed, transactions_removed, splits_removed) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)'
    );

    // ---- 4. All writes in a single atomic transaction ----
    let accountsAdded = 0;
    let transactionsAdded = 0;
    let splitsAdded = 0;
    let commoditiesAdded = 0;

    const doImport = dest.transaction(() => {
      for (const r of srcCommodities) {
        const info = insertCommodity.run(r.guid, r.mnemonic, r.namespace, r.fullname);
        if (info.changes > 0) commoditiesAdded++;
      }

      for (const r of srcAccounts) {
        const info = insertAccount.run(
          r.guid,
          r.name,
          r.account_type,
          r.parent_guid,
          r.code,
          r.description,
          r.placeholder,
          r.commodity_guid
        );
        if (info.changes > 0) accountsAdded++;
      }

      for (const r of srcTxns) {
        const info = insertTx.run(r.guid, r.post_date, r.description);
        if (info.changes > 0) transactionsAdded++;
      }

      for (const r of srcSplits) {
        const info = insertSplit.run(
          r.guid,
          r.tx_guid,
          r.account_guid,
          r.value_num,
          r.value_denom,
          r.quantity_num,
          r.quantity_denom
        );
        if (info.changes > 0) splitsAdded++;
      }

      for (const r of srcPrices) {
        insertPrice.run(
          r.guid,
          r.commodity_guid,
          r.currency_guid,
          r.value_num,
          r.value_denom,
          r.date
        );
      }

      insertLog.run(
        filename,
        new Date().toISOString(),
        accountsAdded,
        transactionsAdded,
        splitsAdded,
        commoditiesAdded,
        accountsRemoved,
        transactionsRemoved,
        splitsRemoved
      );
    });

    doImport();

    // ---- 5. Read post-import stats ----
    const summary = parseGnucashDb(dest);

    const totalAccounts =
      (dest.prepare('SELECT COUNT(*) as cnt FROM accounts').get() as { cnt: number }).cnt ?? 0;
    const totalTransactions =
      (dest.prepare('SELECT COUNT(*) as cnt FROM transactions').get() as { cnt: number }).cnt ?? 0;

    return {
      accountsAdded,
      transactionsAdded,
      splitsAdded,
      commoditiesAdded,
      totalAccounts,
      totalTransactions,
      summary
    };
  } finally {
    source.close();
    dest.close();
  }
}
