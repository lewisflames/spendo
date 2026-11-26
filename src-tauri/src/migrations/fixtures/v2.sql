-- Frozen version 2 book (user_version 2): the schema exactly as db::SCHEMA
-- created it, with a real-shaped household: two currencies, two stocks bought
-- with fees, a partial sale, a dividend with withholding, payees, tags, a
-- finished reconciliation, schedules with skips and options, and budgets with
-- cents. Never edit: add a new fixture per version.
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


INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES
	('c-eur', 'EUR', 'CURRENCY', 'Euro'),
	('c-usd', 'USD', 'CURRENCY', 'US Dollar'),
	('c-vwce', 'VWCE', 'XETRA', 'Vanguard FTSE All-World'),
	('c-aapl', 'AAPL', 'NASDAQ', 'Apple Inc');
INSERT INTO accounts (guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid) VALUES
	('a-root', 'Root Account', 'ROOT', NULL, NULL, NULL, 0, 'c-eur'),
	('a-assets', 'Assets', 'ASSET', 'a-root', NULL, NULL, 1, 'c-eur'),
	('a-bank', 'Bank', 'BANK', 'a-assets', '1100', 'Current account', 0, 'c-eur'),
	('a-cash', 'Wallet', 'CASH', 'a-assets', NULL, NULL, 0, 'c-eur'),
	('a-usd', 'Dollar account', 'BANK', 'a-assets', NULL, NULL, 0, 'c-usd'),
	('a-broker', 'Broker', 'ASSET', 'a-assets', NULL, NULL, 1, 'c-eur'),
	('a-vwce', 'VWCE', 'STOCK', 'a-broker', NULL, NULL, 0, 'c-vwce'),
	('a-aapl', 'AAPL', 'STOCK', 'a-broker', NULL, NULL, 0, 'c-aapl'),
	('a-liabilities', 'Liabilities', 'LIABILITY', 'a-root', NULL, NULL, 1, 'c-eur'),
	('a-card', 'Card', 'CREDIT', 'a-liabilities', NULL, NULL, 0, 'c-eur'),
	('a-income', 'Income', 'INCOME', 'a-root', NULL, NULL, 1, 'c-eur'),
	('a-salary', 'Salary', 'INCOME', 'a-income', NULL, NULL, 0, 'c-eur'),
	('a-dividends', 'Dividends', 'INCOME', 'a-income', NULL, NULL, 0, 'c-eur'),
	('a-expenses', 'Expenses', 'EXPENSE', 'a-root', NULL, NULL, 1, 'c-eur'),
	('a-food', 'Food', 'EXPENSE', 'a-expenses', NULL, NULL, 0, 'c-eur'),
	('a-groceries', 'Groceries', 'EXPENSE', 'a-food', NULL, NULL, 0, 'c-eur'),
	('a-rent', 'Rent', 'EXPENSE', 'a-expenses', NULL, NULL, 0, 'c-eur'),
	('a-fees', 'Fees', 'EXPENSE', 'a-expenses', NULL, NULL, 0, 'c-eur'),
	('a-tax', 'Withholding', 'EXPENSE', 'a-expenses', NULL, NULL, 0, 'c-eur'),
	('a-equity', 'Equity', 'EQUITY', 'a-root', NULL, NULL, 1, 'c-eur'),
	('a-opening', 'Opening balances', 'EQUITY', 'a-equity', NULL, NULL, 0, 'c-eur');
INSERT INTO transactions (guid, post_date, description, num, notes) VALUES
	('t-open', '2024-01-01', 'Opening balance', NULL, NULL),
	('t-sal1', '2024-01-31', 'January salary', NULL, NULL),
	('t-rent1', '2024-02-01', 'Rent', '101', NULL),
	('t-groc1', '2024-02-03', 'Weekly shop', NULL, 'paid by card'),
	('t-groc2', '2024-02-10 10:30:00', 'Weekly shop', NULL, NULL),
	('t-cash', '2024-02-12', 'ATM', NULL, NULL),
	('t-buy-vwce', '2024-03-01', 'Buy VWCE', NULL, NULL),
	('t-fx', '2024-03-05', 'To dollars', NULL, NULL),
	('t-buy-aapl', '2024-03-06', 'Buy AAPL', NULL, NULL),
	('t-sal2', '2024-03-31', 'March salary', NULL, NULL),
	('t-sell-vwce', '2024-06-03', 'Sell VWCE', NULL, NULL),
	('t-div', '2024-06-20', 'VWCE dividend', NULL, NULL),
	('t-groc3', '2025-01-15', 'Weekly shop', NULL, NULL),
	('t-card', '2025-01-20', 'Card payment', NULL, NULL);
INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state, reconciled, memo) VALUES
	('s-open-a', 't-open', 'a-bank', 500000, 100, 500000, 100, 'y', 0, NULL),
	('s-open-b', 't-open', 'a-opening', -500000, 100, -500000, 100, 'n', 0, NULL),
	('s-sal1-a', 't-sal1', 'a-bank', 250000, 100, 250000, 100, 'y', 0, NULL),
	('s-sal1-b', 't-sal1', 'a-salary', -250000, 100, -250000, 100, 'n', 0, NULL),
	('s-rent1-a', 't-rent1', 'a-bank', -90000, 100, -90000, 100, 'y', 0, NULL),
	('s-rent1-b', 't-rent1', 'a-rent', 90000, 100, 90000, 100, 'n', 0, NULL),
	('s-groc1-a', 't-groc1', 'a-card', -4537, 100, -4537, 100, 'c', 0, NULL),
	('s-groc1-b', 't-groc1', 'a-groceries', 4537, 100, 4537, 100, 'n', 0, 'fruit'),
	('s-groc2-a', 't-groc2', 'a-bank', -3210, 100, -3210, 100, 'n', 0, NULL),
	('s-groc2-b', 't-groc2', 'a-food', 3210, 100, 3210, 100, 'n', 0, NULL),
	('s-cash-a', 't-cash', 'a-bank', -10000, 100, -10000, 100, 'n', 0, NULL),
	('s-cash-b', 't-cash', 'a-cash', 10000, 100, 10000, 100, 'n', 0, NULL),
	('s-bv-a', 't-buy-vwce', 'a-bank', -100500, 100, -100500, 100, 'n', 0, NULL),
	('s-bv-b', 't-buy-vwce', 'a-vwce', 100000, 100, 10, 1, 'n', 0, NULL),
	('s-bv-c', 't-buy-vwce', 'a-fees', 500, 100, 500, 100, 'n', 0, NULL),
	('s-fx-a', 't-fx', 'a-bank', -50000, 100, -50000, 100, 'n', 0, NULL),
	('s-fx-b', 't-fx', 'a-usd', 50000, 100, 54000, 100, 'n', 0, NULL),
	('s-ba-a', 't-buy-aapl', 'a-usd', -54000, 100, -54000, 100, 'n', 0, NULL),
	('s-ba-b', 't-buy-aapl', 'a-aapl', 54000, 100, 3, 1, 'n', 0, NULL),
	('s-sal2-a', 't-sal2', 'a-bank', 250000, 100, 250000, 100, 'n', 0, NULL),
	('s-sal2-b', 't-sal2', 'a-salary', -250000, 100, -250000, 100, 'n', 0, NULL),
	('s-sv-a', 't-sell-vwce', 'a-bank', 44500, 100, 44500, 100, 'n', 0, NULL),
	('s-sv-b', 't-sell-vwce', 'a-vwce', -45000, 100, -4, 1, 'n', 0, NULL),
	('s-sv-c', 't-sell-vwce', 'a-fees', 500, 100, 500, 100, 'n', 0, NULL),
	('s-div-a', 't-div', 'a-bank', 1275, 100, 1275, 100, 'n', 0, NULL),
	('s-div-b', 't-div', 'a-vwce', 0, 100, 0, 1, 'n', 0, NULL),
	('s-div-c', 't-div', 'a-tax', 225, 100, 225, 100, 'n', 0, NULL),
	('s-div-d', 't-div', 'a-dividends', -1500, 100, -1500, 100, 'n', 0, NULL),
	('s-groc3-a', 't-groc3', 'a-card', -6789, 100, -6789, 100, 'n', 0, NULL),
	('s-groc3-b', 't-groc3', 'a-groceries', 6789, 100, 6789, 100, 'n', 0, NULL),
	('s-card-a', 't-card', 'a-bank', -11326, 100, -11326, 100, 'n', 0, NULL),
	('s-card-b', 't-card', 'a-card', 11326, 100, 11326, 100, 'n', 0, NULL);
INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date) VALUES
	('p-vwce-1', 'c-vwce', 'c-eur', 10000, 100, '2024-03-01'),
	('p-vwce-2', 'c-vwce', 'c-eur', 11250, 100, '2024-06-03'),
	('p-vwce-3', 'c-vwce', 'c-eur', 12010, 100, '2025-01-31'),
	('p-aapl-1', 'c-aapl', 'c-usd', 18000, 100, '2024-03-06'),
	('p-aapl-2', 'c-aapl', 'c-usd', 23050, 100, '2025-01-31'),
	('p-usd-1', 'c-usd', 'c-eur', 9259, 10000, '2024-03-05'),
	('p-usd-2', 'c-usd', 'c-eur', 9615, 10000, '2025-01-31');
INSERT INTO budgets (guid, account_guid, year, amount, created_at) VALUES
	('b-food-24', 'a-food', 2024, 3000.5, '2024-01-01T00:00:00Z'),
	('b-food-25', 'a-food', 2025, 1234.56, '2025-01-01T00:00:00Z'),
	('b-rent-24', 'a-rent', 2024, 10800, '2024-01-01T00:00:00Z');
INSERT INTO payees (guid, name, created_at) VALUES
	('p-mercado', 'Mercado', '2024-02-03T00:00:00Z'),
	('p-landlord', 'Landlord', '2024-02-01T00:00:00Z'),
	('p-employer', 'Employer', '2024-01-31T00:00:00Z');
INSERT INTO transaction_payees (tx_guid, payee_guid) VALUES
	('t-groc1', 'p-mercado'), ('t-groc2', 'p-mercado'), ('t-groc3', 'p-mercado'),
	('t-rent1', 'p-landlord'), ('t-sal1', 'p-employer'), ('t-sal2', 'p-employer');
INSERT INTO tags (guid, name, created_at) VALUES ('g-home', 'home', '2024-02-01T00:00:00Z');
INSERT INTO transaction_tags (tx_guid, tag_guid) VALUES ('t-rent1', 'g-home'), ('t-groc1', 'g-home');
INSERT INTO reconcile_batches (guid, account_guid, finished_at, through_date) VALUES
	('rb-1', 'a-bank', '2024-02-05T00:00:00Z', '2024-02-01');
INSERT INTO reconcile_batch_splits (batch_guid, split_guid) VALUES
	('rb-1', 's-open-a'), ('rb-1', 's-sal1-a'), ('rb-1', 's-rent1-a');
INSERT INTO attachments (guid, tx_guid, filename, mime_type, byte_size, data, created_at) VALUES
	('at-1', 't-rent1', 'receipt.pdf', 'application/pdf', 2, X'2550', '2024-02-01T00:00:00Z');
INSERT INTO recurring_schedules (guid, name, frequency, interval_mult, start_date, end_date, max_occurrences, occurrences_posted, next_due_date, active, template_json, created_at) VALUES
	('r-rent', 'Rent', 'monthly', 1, '2024-02-01', NULL, 12, 1, '2024-05-01', 1, '{"description":"Rent","num":null,"notes":null,"payee":"Landlord","splits":[{"accountGuid":"a-rent","amount":900.0,"quantity":null,"memo":null},{"accountGuid":"a-bank","amount":-900.0,"quantity":null,"memo":null}]}', '2024-01-20T00:00:00Z'),
	('r-laptop', 'Laptop', 'monthly', 1, '2024-01-31', NULL, 24, 0, '2024-01-31', 0, '{"description":"Laptop","num":null,"notes":null,"payee":null,"splits":[{"accountGuid":"a-fees","amount":1200.0,"quantity":null,"memo":null},{"accountGuid":"a-bank","amount":-1200.0,"quantity":null,"memo":null}]}', '2024-01-20T00:00:00Z'),
	('r-gym', 'Gym', 'weekly', 2, '2024-03-01', '2024-12-31', NULL, 0, '2024-03-01', 1, '{"description":"Gym","num":null,"notes":null,"payee":null,"splits":[{"accountGuid":"a-fees","amount":20.0,"quantity":null,"memo":null},{"accountGuid":"a-cash","amount":-20.0,"quantity":null,"memo":null}]}', '2024-01-20T00:00:00Z');
INSERT INTO schedule_skips (schedule_guid, skipped_count) VALUES ('r-rent', 2);
INSERT INTO schedule_options (schedule_guid, auto_post, spread_total) VALUES
	('r-rent', 1, 0), ('r-laptop', 0, 1);
INSERT INTO dividend_links (account_guid, commodity_guid) VALUES ('a-dividends', 'c-vwce');
INSERT INTO tax_roles (account_guid, role) VALUES ('a-salary', 'employment_income');
INSERT INTO symbol_overrides (commodity_guid, symbol) VALUES ('c-vwce', 'VWCE.DE');
INSERT INTO app_settings (key, value) VALUES ('cost_basis_method', 'fifo');
