-- Frozen version 1 book (user_version 1, or 0 before versioning): the
-- schema exactly as db::SCHEMA created it, plus the legacy tables early
-- builds left behind and a few rows. Never edit: add a new fixture per version.
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
	description TEXT,
	num TEXT,
	notes TEXT
);

CREATE TABLE IF NOT EXISTS splits (
	guid TEXT PRIMARY KEY,
	tx_guid TEXT NOT NULL,
	account_guid TEXT NOT NULL,
	value_num INTEGER NOT NULL,
	value_denom INTEGER NOT NULL,
	quantity_num INTEGER NOT NULL,
	quantity_denom INTEGER NOT NULL,
	reconcile_state TEXT,
	reconciled INTEGER DEFAULT 0,
	memo TEXT
);

CREATE TABLE IF NOT EXISTS prices (
	guid TEXT PRIMARY KEY,
	commodity_guid TEXT NOT NULL,
	currency_guid TEXT NOT NULL,
	value_num INTEGER NOT NULL,
	value_denom INTEGER NOT NULL,
	date TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_transactions_post_date ON transactions(post_date);
CREATE INDEX IF NOT EXISTS idx_transactions_post_date_guid ON transactions(post_date, guid);
CREATE INDEX IF NOT EXISTS idx_splits_tx_guid ON splits(tx_guid);
CREATE INDEX IF NOT EXISTS idx_splits_account_guid ON splits(account_guid);
CREATE INDEX IF NOT EXISTS idx_accounts_account_type ON accounts(account_type);
CREATE INDEX IF NOT EXISTS idx_accounts_parent_guid ON accounts(parent_guid);
CREATE INDEX IF NOT EXISTS idx_prices_commodity_guid_date ON prices(commodity_guid, date);
CREATE INDEX IF NOT EXISTS idx_prices_currency_guid_date ON prices(currency_guid, date);
CREATE INDEX IF NOT EXISTS idx_commodities_mnemonic ON commodities(mnemonic);

CREATE TABLE IF NOT EXISTS budgets (
	guid TEXT PRIMARY KEY,
	account_guid TEXT NOT NULL REFERENCES accounts(guid) ON DELETE CASCADE,
	year INTEGER NOT NULL,
	amount REAL NOT NULL,
	created_at TEXT NOT NULL,
	UNIQUE (account_guid, year)
);

CREATE TABLE IF NOT EXISTS refresh_log (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	refreshed_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_budgets_account_year ON budgets(account_guid, year);

CREATE TABLE IF NOT EXISTS symbol_overrides (
	commodity_guid TEXT PRIMARY KEY,
	symbol TEXT NOT NULL
);

-- Files attached to a transaction. Bytes live in the book itself
-- so the `.spendo` file stays portable (Save As copies them via VACUUM INTO).
-- Up to one `MAX_ATTACHMENTS_PER_TX` rows per transaction (enforced in
-- `attachments.rs`, not by schema); adding files accumulates. Deleting a
-- transaction removes its attachments through the FK cascade.
CREATE TABLE IF NOT EXISTS attachments (
	guid TEXT PRIMARY KEY,
	tx_guid TEXT NOT NULL REFERENCES transactions(guid) ON DELETE CASCADE,
	filename TEXT NOT NULL,
	mime_type TEXT NOT NULL,
	byte_size INTEGER NOT NULL,
	data BLOB NOT NULL,
	created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_attachments_tx_guid ON attachments(tx_guid);

-- Recurring transaction schedules (Scheduled feature). Future intent lives
-- here, never in `transactions`: rows there must stay within the visibility
-- window (`VISIBLE_TX_FILTER`) and the manual future-date block. A schedule
-- holds a serialized `TxInput`-shaped template; posting materializes one real
-- transaction per due occurrence and advances `next_due_date`. `NULL`
-- `next_due_date` means completed (end date / occurrence cap reached).
CREATE TABLE IF NOT EXISTS recurring_schedules (
	guid TEXT PRIMARY KEY,
	name TEXT NOT NULL,
	frequency TEXT NOT NULL,
	interval_mult INTEGER NOT NULL DEFAULT 1,
	start_date TEXT NOT NULL,
	end_date TEXT,
	max_occurrences INTEGER,
	occurrences_posted INTEGER NOT NULL DEFAULT 0,
	next_due_date TEXT,
	active INTEGER NOT NULL DEFAULT 1,
	template_json TEXT NOT NULL,
	created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_schedules_active_due ON recurring_schedules(active, next_due_date);

-- Skip counter for recurring schedules, in a side table so the core row keeps
-- its original columns and older books pick this up on open. A missing
-- row means nothing has been skipped; rows cascade with their schedule.
CREATE TABLE IF NOT EXISTS schedule_skips (
	schedule_guid TEXT PRIMARY KEY REFERENCES recurring_schedules(guid) ON DELETE CASCADE,
	skipped_count INTEGER NOT NULL DEFAULT 0
);

-- Per-schedule posting options, kept beside `recurring_schedules` for the same
-- reason as `schedule_skips`. `auto_post` posts due occurrences when the book
-- opens; `spread_total` treats the template amounts as a total divided across
-- `max_occurrences` (e.g. depreciating a device). A missing row means both off.
CREATE TABLE IF NOT EXISTS schedule_options (
	schedule_guid TEXT PRIMARY KEY REFERENCES recurring_schedules(guid) ON DELETE CASCADE,
	auto_post INTEGER NOT NULL DEFAULT 0,
	spread_total INTEGER NOT NULL DEFAULT 0
);

-- Finished reconciliation batches. Persisting the membership (not inferring it
-- from transaction dates) is what lets Undo last statement reopen exactly the
-- splits a Finish promoted, even when one was dated before the previous
-- batch's last transaction. Splits reconciled outside Spendo (imported books)
-- have no batch and can't be reopened. Rows are deleted by reopen; ON DELETE
-- CASCADE clears mappings.
CREATE TABLE IF NOT EXISTS reconcile_batches (
	guid TEXT PRIMARY KEY,
	account_guid TEXT NOT NULL REFERENCES accounts(guid) ON DELETE CASCADE,
	finished_at TEXT NOT NULL,
	through_date TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reconcile_batches_account ON reconcile_batches(account_guid, finished_at);

CREATE TABLE IF NOT EXISTS reconcile_batch_splits (
	batch_guid TEXT NOT NULL REFERENCES reconcile_batches(guid) ON DELETE CASCADE,
	split_guid TEXT NOT NULL REFERENCES splits(guid) ON DELETE CASCADE,
	PRIMARY KEY (batch_guid, split_guid)
);

-- Saved CSV import mapping presets, keyed by a hash of the normalized header
-- row. Book-scoped convenience config, dropped by `clear_db` like any other
-- table.
CREATE TABLE IF NOT EXISTS csv_import_presets (
	signature TEXT PRIMARY KEY,
	name TEXT NOT NULL,
	mapping_json TEXT NOT NULL,
	updated_at TEXT NOT NULL
);

-- Description-match rules that pick the category (offset account) for a CSV
-- import row. User config, so account deletion is guarded instead of
-- cascading silently.
CREATE TABLE IF NOT EXISTS import_rules (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	priority INTEGER NOT NULL,
	match_type TEXT NOT NULL,
	pattern TEXT NOT NULL,
	account_guid TEXT NOT NULL REFERENCES accounts(guid) ON DELETE CASCADE,
	created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_import_rules_priority ON import_rules(priority, id);

-- Target allocation per commodity for the portfolio rebalance calculator.
-- Book-scoped user config like `import_rules`. A target dies with its
-- commodity through the FK cascade.
CREATE TABLE IF NOT EXISTS allocation_targets (
	commodity_guid TEXT PRIMARY KEY REFERENCES commodities(guid) ON DELETE CASCADE,
	target_pct REAL NOT NULL
);

-- Income accounts whose postings are dividends of one holding, for books
-- that don't record a zero-amount split on the stock account.
-- User config like `import_rules`: account deletion is guarded instead of
-- cascading silently.
CREATE TABLE IF NOT EXISTS dividend_links (
	account_guid TEXT PRIMARY KEY REFERENCES accounts(guid) ON DELETE CASCADE,
	commodity_guid TEXT NOT NULL REFERENCES commodities(guid) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dividend_links_commodity ON dividend_links(commodity_guid);

-- Free-form labels on whole transactions (a trip, a project, deductible).
-- Names are unique ignoring case. A tag outlives its last transaction until
-- it is deleted from Settings; links die with either side (FK cascade, so
-- transaction deletes clean them up).
CREATE TABLE IF NOT EXISTS tags (
	guid TEXT PRIMARY KEY,
	name TEXT NOT NULL UNIQUE COLLATE NOCASE,
	created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS transaction_tags (
	tx_guid TEXT NOT NULL REFERENCES transactions(guid) ON DELETE CASCADE,
	tag_guid TEXT NOT NULL REFERENCES tags(guid) ON DELETE CASCADE,
	PRIMARY KEY (tx_guid, tag_guid)
);

CREATE INDEX IF NOT EXISTS idx_transaction_tags_tag ON transaction_tags(tag_guid);

-- Who a transaction was with (a shop, a person, an employer); at most one per
-- transaction. Kept beside `transactions` like `schedule_skips` so the core
-- row keeps its original columns and older books pick this up on open.
-- Names are unique ignoring case; links die with either side (FK cascade, so
-- deleting a payee only clears it from its transactions).
CREATE TABLE IF NOT EXISTS payees (
	guid TEXT PRIMARY KEY,
	name TEXT NOT NULL UNIQUE COLLATE NOCASE,
	created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS transaction_payees (
	tx_guid TEXT PRIMARY KEY REFERENCES transactions(guid) ON DELETE CASCADE,
	payee_guid TEXT NOT NULL REFERENCES payees(guid) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_transaction_payees_payee ON transaction_payees(payee_guid);

-- Tax role per account for the tax summary; descendants inherit the nearest
-- ancestor's role. User config like `import_rules`: account deletion is
-- guarded instead of cascading silently.
CREATE TABLE IF NOT EXISTS tax_roles (
	account_guid TEXT PRIMARY KEY REFERENCES accounts(guid) ON DELETE CASCADE,
	role TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS app_settings (
	key TEXT PRIMARY KEY,
	value TEXT NOT NULL
);

-- Leftovers from early builds (dropped by migration 2).
CREATE TABLE change_log (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	changed_at TEXT NOT NULL,
	summary TEXT NOT NULL
);
CREATE TABLE change_snapshots (
	entry_id INTEGER NOT NULL REFERENCES change_log(id) ON DELETE CASCADE,
	table_name TEXT NOT NULL,
	row_json TEXT NOT NULL
);
CREATE TABLE manual_prices (
	commodity_guid TEXT NOT NULL,
	date TEXT NOT NULL,
	value REAL NOT NULL
);

INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES
	('c-eur', 'EUR', 'CURRENCY', 'Euro');
INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid) VALUES
	('a-root', 'Root Account', 'ROOT', NULL, 0, 'c-eur'),
	('a-assets', 'Assets', 'ASSET', 'a-root', 1, 'c-eur'),
	('a-bank', 'Bank', 'BANK', 'a-assets', 0, 'c-eur'),
	('a-expenses', 'Expenses', 'EXPENSE', 'a-root', 1, 'c-eur'),
	('a-food', 'Food', 'EXPENSE', 'a-expenses', 0, 'c-eur'),
	('a-income', 'Income', 'INCOME', 'a-root', 1, 'c-eur'),
	('a-liabilities', 'Liabilities', 'LIABILITY', 'a-root', 1, 'c-eur'),
	('a-equity', 'Equity', 'EQUITY', 'a-root', 1, 'c-eur');
INSERT INTO transactions (guid, post_date, description) VALUES
	('t-1', '2024-01-05', 'Groceries'),
	('t-2', '2024-01-12', 'Groceries');
INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state) VALUES
	('s-1a', 't-1', 'a-bank', -2550, 100, -2550, 100, 'y'),
	('s-1b', 't-1', 'a-food', 2550, 100, 2550, 100, 'n'),
	('s-2a', 't-2', 'a-bank', -1200, 100, -1200, 100, 'n'),
	('s-2b', 't-2', 'a-food', 1200, 100, 1200, 100, 'n');
INSERT INTO budgets (guid, account_guid, year, amount, created_at) VALUES
	('b-1', 'a-food', 2024, 3000, '2024-01-01T00:00:00Z');
INSERT INTO payees (guid, name, created_at) VALUES
	('p-1', 'Mercado', '2024-01-05T00:00:00Z');
INSERT INTO transaction_payees (tx_guid, payee_guid) VALUES ('t-1', 'p-1');
INSERT INTO tags (guid, name, created_at) VALUES ('g-1', 'home', '2024-01-05T00:00:00Z');
INSERT INTO transaction_tags (tx_guid, tag_guid) VALUES ('t-2', 'g-1');
INSERT INTO change_log (changed_at, summary) VALUES ('2024-01-05T00:00:00Z', 'created t-1');
INSERT INTO change_snapshots (entry_id, table_name, row_json) VALUES (1, 'transactions', '{}');
INSERT INTO manual_prices (commodity_guid, date, value) VALUES ('c-eur', '2024-01-01', 1);
