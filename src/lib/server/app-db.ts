import Database from 'better-sqlite3';
import { join } from 'node:path';
import { existsSync, mkdirSync } from 'node:fs';

const DATA_DIR = process.env['DATA_DIR'] ?? join(process.cwd(), 'data');
const DB_PATH = join(DATA_DIR, 'spendo.db');

function ensureDataDir() {
  if (!existsSync(DATA_DIR)) {
    mkdirSync(DATA_DIR, { recursive: true });
  }
}

export function getAppDbPath(): string {
  return DB_PATH;
}

const SCHEMA = `
CREATE TABLE IF NOT EXISTS commodities (
	guid TEXT PRIMARY KEY,
	mnemonic TEXT NOT NULL,
	namespace TEXT NOT NULL,
	fullname TEXT
);

CREATE TABLE IF NOT EXISTS accounts (
	guid TEXT PRIMARY KEY,
	name TEXT NOT NULL,
	account_type TEXT NOT NULL,
	parent_guid TEXT,
	code TEXT,
	description TEXT,
	placeholder INTEGER DEFAULT 0,
	commodity_guid TEXT
);

CREATE TABLE IF NOT EXISTS transactions (
	guid TEXT PRIMARY KEY,
	post_date TEXT NOT NULL,
	description TEXT
);

CREATE TABLE IF NOT EXISTS splits (
	guid TEXT PRIMARY KEY,
	tx_guid TEXT NOT NULL,
	account_guid TEXT NOT NULL,
	value_num INTEGER NOT NULL,
	value_denom INTEGER NOT NULL,
	quantity_num INTEGER NOT NULL,
	quantity_denom INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS prices (
	guid TEXT PRIMARY KEY,
	commodity_guid TEXT NOT NULL,
	currency_guid TEXT NOT NULL,
	value_num INTEGER NOT NULL,
	value_denom INTEGER NOT NULL,
	date TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS import_log (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	filename TEXT NOT NULL,
	imported_at TEXT NOT NULL,
	accounts_added INTEGER DEFAULT 0,
	transactions_added INTEGER DEFAULT 0,
	splits_added INTEGER DEFAULT 0,
	commodities_added INTEGER DEFAULT 0,
	accounts_removed INTEGER DEFAULT 0,
	transactions_removed INTEGER DEFAULT 0,
	splits_removed INTEGER DEFAULT 0
);
`;

export function initAppDb(db: Database.Database) {
  db.exec(SCHEMA);
  // Migrate existing databases to add *_removed columns (added in v2)
  for (const col of ['accounts_removed', 'transactions_removed', 'splits_removed']) {
    try {
      db.exec(`ALTER TABLE import_log ADD COLUMN ${col} INTEGER DEFAULT 0`);
    } catch (err) {
      if (!(err instanceof Error && err.message.includes('duplicate column'))) {
        throw err;
      }
    }
  }
}

export function getAppDb(): Database.Database {
  ensureDataDir();
  const db = new Database(DB_PATH);
  db.pragma('journal_mode = WAL');
  initAppDb(db);
  return db;
}

export function clearAppDb() {
  const db = getAppDb();
  try {
    db.exec(`
      DELETE FROM prices;
      DELETE FROM splits;
      DELETE FROM transactions;
      DELETE FROM accounts;
      DELETE FROM commodities;
      DELETE FROM import_log;
    `);
  } finally {
    db.close();
  }
}
