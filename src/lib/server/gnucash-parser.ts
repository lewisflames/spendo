import type Database from 'better-sqlite3';

export interface GnucashSummary {
  accountsCount: number;
  accountsByType: Record<string, number>;
  transactionsCount: number;
  dateRange: { from: string; to: string } | null;
  currency: string;
}

export function parseGnucashDb(db: Database.Database): GnucashSummary {
  const accountsRow = db.prepare('SELECT COUNT(*) as cnt FROM accounts').get() as
    { cnt: number } | undefined;
  const accountsCount = accountsRow?.cnt ?? 0;

  const byType = db
    .prepare('SELECT account_type, COUNT(*) as cnt FROM accounts GROUP BY account_type')
    .all() as { account_type: string; cnt: number }[];
  const accountsByType: Record<string, number> = {};
  for (const row of byType) {
    accountsByType[row.account_type] = row.cnt;
  }

  const txRow = db.prepare('SELECT COUNT(*) as cnt FROM transactions').get() as
    { cnt: number } | undefined;
  const transactionsCount = txRow?.cnt ?? 0;

  const dateRow = db
    .prepare('SELECT MIN(post_date) as first_date, MAX(post_date) as last_date FROM transactions')
    .get() as { first_date: string | null; last_date: string | null } | undefined;
  const dateRange =
    dateRow?.first_date && dateRow?.last_date
      ? { from: dateRow.first_date, to: dateRow.last_date }
      : null;

  const currencyRow = db
    .prepare("SELECT mnemonic FROM commodities WHERE namespace = 'CURRENCY' LIMIT 1")
    .get() as { mnemonic: string } | undefined;
  const currency = currencyRow?.mnemonic ?? 'Unknown';

  return { accountsCount, accountsByType, transactionsCount, dateRange, currency };
}
