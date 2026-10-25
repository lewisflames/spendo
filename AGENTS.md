# spendo

Tauri v2 desktop app: static SvelteKit SPA (Svelte 5 runes, Tailwind v4, `@sveltejs/adapter-static`, no SSR) over a Rust (`rusqlite`) backend. Local-first personal finance manager with portable `.spendo` books; GnuCash is one import source, not the app's center.

## Commands

| Action          | Command                                               |
| --------------- | ----------------------------------------------------- |
| Dev server      | `bun run dev`                                         |
| Build           | `bun run build`                                       |
| Type-check      | `bun run check`                                       |
| Test (frontend) | `bun run test` (vitest; colocated `src/**/*.test.ts`) |
| Test (UI)       | `bun run test:e2e` (Playwright; `e2e/*.spec.ts`)      |
| Lint            | `bun run lint`                                        |
| Format          | `bun run format`                                      |
| Tauri dev       | `bun run tauri:dev` (generates icons first)           |
| Tauri build     | `bun run tauri:build` (generates icons first)         |

Frontend work is done when `bun run check`, `bun run lint` and `bun run test` pass; Rust unit tests run with `cargo test` in `src-tauri/`.

## Code style

- Svelte 5 runes: `$props()`, `$derived()`, `$state()`; events as `onclick`; children via `{@render children()}`.
- Prettier: 2-space indent, single quotes, no trailing commas, print width 100, Tailwind class sorting.
- Tailwind v4 via the Vite plugin (`@import 'tailwindcss'` in `src/app.css`, no PostCSS).
- TypeScript strict; top-level `await` is allowed in components (`experimental.async`).
- Icons: Lucide (`@lucide/svelte`).
- Rust: `serde` with `#[serde(rename_all = "camelCase")]` on all serialized response structs.
- shadcn-svelte components live in `$lib/components/ui/` (CLI-managed); hand-written ones in `$lib/components/layout/` and `$lib/components/shared/`. `cn()` comes from `$lib/utils.ts`.

## Architecture

- All backend calls go through `invoke()` via `$lib/tauri.ts` — no `+server.ts` / `+page.server.ts`.
- One active `.spendo` book opened in place. Every connection swap bumps `book_generation` while the DB lock is held; long-running network writes capture the generation before fetching and abort if it changed. Books carry `PRAGMA application_id` (`SPENDO_APP_ID`). No migrations by design (pre-release): `SCHEMA` is the source of truth, so schema changes mean a reset (Settings → Clear all data or delete the book) plus re-import.
- Routes contain only route files; page components/types live in `$lib/features/<domain>/`, domain utilities in `$lib/utils/`.
- Store convention: `.svelte.ts` modules using `$state()` wrapped in `{ value }` — every store, including scalar flags.
- Every page needs the Rust backend: develop with `bun run tauri:dev`. A plain browser (`bun run dev`) has no backend, so `invoke()` calls reject.
- Playwright specs run against `bun run dev` and stub IPC with `mockBackend(page, { command: result })` from `e2e/fixtures.ts`; unmocked commands reject with `unmocked command: <name>`.

## Data / imports

- GnuCash import accepts only `.gnucash` (SQLite) files and merges into the active book — never opens in place. Always preview → confirm → merge: the preview hashes the file and reports add/update/delete counts without writing; the import re-hashes and rejects a stale file. The source opens strictly read-only, and `prices` are never imported from it. Post dates are stored date-only like hand-entered rows: GnuCash's UTC timestamp becomes the local calendar day (`normalize_post_date`), never a truncation.
- The orphan sweep deletes rows absent from the source unless `deleteOrphans: false`. It protects the roots of categories the source lacks and accounts carrying budgets or referenced by import rules/schedules. Source scheduled transactions aren't imported: their template accounts/transactions (and the `template` commodity) are excluded from the merge and the preview only counts them (`schedulesIgnored`). A source with no accounts is rejected.
- CSV import (`/import/csv`) is detect → map → preview → commit for `.csv`/`.txt`/`.tsv`; the detect SHA-256 is echoed through preview/commit, so a changed file aborts. Commits remember the mapping per header signature (`csv_import_presets`) unless `rememberMapping: false`.
- Import rules pick the category for the first rule (priority order) whose pattern `contains`/`equals` the row's description; the mapping step's offset account is the fallback. Rules apply inside `parse_file_rows`, so preview and commit agree.
- CSV exports are written in Rust in one command each (`export_transactions_csv`, `export_accounts_csv`) — no full-book payload crosses IPC. The transactions export deliberately ignores the visibility window so future-dated rows survive, and a round trip back flags them as errors.
- Prices come from Yahoo only (`yahoo.rs` + `fx.rs`), never from the source book; per-commodity overrides live in `symbol_overrides`. Only one candidate symbol wins per commodity (the first with a history, in derivation order) and prices are written per commodity GUID — never resolved by mnemonic, which can repeat across namespaces.
- A finished reconciliation records exactly the splits it promoted in `reconcile_batches`/`reconcile_batch_splits`; Undo last statement reopens that batch; splits reconciled outside Spendo (imported books) only show their last date and can't be reopened. Skipped schedule occurrences live in `schedule_skips` and count toward `max_occurrences` like posted ones, so a skip-only schedule can complete and an edit cannot replay skipped dates. Per-schedule options live in `schedule_options` (row only while one is on): `auto_post` schedules are posted by `post_due_schedules`, which the layout runs once per opened book (`auto-post.ts`); `spread_total` treats the template amounts as a total split across `max_occurrences` by cumulative cent rounding, so the slices sum exactly and each balances.
- The transit check (`transit.rs`) pairs legs in the book's clearing account (book setting `transit_account`, toggled from the account page's overflow menu) with an opposite-value leg at most 14 days apart; what's left shows on that account's page.
- Any export/download target goes through `AppState::ensure_not_active_book`: writing an export onto the live `.spendo` file would truncate it.
- Backups (`backups.rs`) are `VACUUM INTO` snapshots in `<app data>/backups/<stem>-<path hash>/`, named `<UTC stamp>Z_<reason>.spendo`. A daily one runs on open and hourly (book setting `auto_backup`, skipped for books without transactions); bulk changes (GnuCash import, CSV commit, bulk transaction edits, Clear all data, restore) call `backups::before_change` inside their locked closure and abort if it fails. Retention keeps 7 daily and 10 safety snapshots and never prunes manual ones. The snapshot's book always comes from `Connection::path()` under the lock, never `db_path`. Restore copies pages into the live connection with the SQLite backup API and bumps `book_generation`; the frontend goes through `restoreBackupFile` in `book-actions.ts`.
- Investment returns replay lots in `lots.rs` on every request (never stored), FIFO or average cost per book setting `cost_basis_method`. `portfolio.rs::load_return_inputs` nets investment splits per (transaction, commodity), so moves between accounts aren't sales, and adds the transaction's EXPENSE splits (fees) to its trades by traded value. `cost_basis` stays net cash invested; `open_cost_basis`/`unrealized_gain`/`realized_gain` come from the lots, and XIRR uses the same fee-inclusive events.
- Dividends (`dividends.rs`) are income transactions that touch a holding's investment account with only a zero-amount split (GnuCash's convention) or a share purchase (reinvestment); one with a sale or a value-only split is a gain booking, not a dividend. Transactions without investment splits count through `dividend_links` (income account → commodity, guarded on account delete and in the orphan sweep like import rules). Amounts are net of the transaction's EXPENSE splits (withholding), which are therefore not added to share cost; dividends feed `dividends`, `total_return` and XIRR.
- Realized gains (`/portfolio/gains`, sidebar group Investments with Portfolio) come from `portfolio::realized_gains_inner`, one row per lot a sale consumed, and the CSV export (`export_realized_gains_csv`) reuses it so the file always matches the page. `/transactions/new?dividend=<commodityGuid>` prefills GnuCash's dividend shape (zero split on the holding's largest account, first linked dividend account) through `TransactionForm`'s `prefill` prop.
- The spending report (`/reports/spending`, `spending.rs`) reuses the P&L monthly buckets and month-end FX, so its totals match the P&L. Categories are the children of `?category=` (default: the single expense root's children, or every root when there are several), each rolling up its descendants; a parent's own splits list as a separate `ownSplits` row. The chart gives the eight largest their own `--series-N` hue in fixed order and folds the rest into Other (`spending-utils.ts`); never add generated hues.
- Bulk transaction edits (`bulk_edit_transactions`: delete, move splits between same-commodity accounts, rename) target explicit GUIDs or "all matching" list filters plus the `expectedCount` the user saw, refused when the count changed. Actions that rewrite splits skip cleared/reconciled ones and report `skippedLocked`; renames touch no splits, so they apply to locked transactions too.
- Tags (`tags.rs`) label whole transactions, not splits: `tags` (name unique ignoring case) + `transaction_tags` (FK cascade on both sides, so deletes and the orphan sweep clean links up). Callers send tag names in `TxInput.tags` and `set_transaction_tags` resolves or creates them; `None` leaves an edited transaction's tags alone. Lists filter with `Filters.tagGuid` (`?tag=` on `/transactions`), search matches tag names through `push_tx_date_search`, and bulk `addTag`/`removeTag` skip no locked rows (tags don't touch splits). Renaming onto another tag's name merges them. Name rules (40 chars, no commas, 20 per transaction) live in `tags.rs`, mirrored in `$lib/features/tags/tag-utils.ts`.
- Payees (`payees.rs`) name who a transaction was with: `payees` (name unique ignoring case) + `transaction_payees` (one row per transaction, FK cascade on both sides, so deleting a payee only clears it). Callers send a name in `TxInput.payee` (`None` leaves an edited transaction's payee alone, blank clears it) and `set_transaction_payee` resolves or creates it; schedule templates carry `payee` and posting applies it. Lists filter with `Filters.payeeGuid` (`?payee=` on `/transactions`, a removable chip), search matches payee names, bulk `setPayee` skips no locked rows, and renaming onto another payee's name merges them (Settings → Payees). The transactions export appends a `Payee` column. Keep the payee out of `notes` and split memos.
- The tax summary (`/reports/tax?year=`, `tax.rs`) sums the year's activity per tax role (`tax_roles`, inherited by sub-accounts; income roles only on INCOME accounts, the rest on EXPENSE) from the cash-flow monthly buckets, so totals match that report. `tax_settlement` postings count for the year before (`SETTLEMENT_LAG_YEARS`): keep annual return payments and refunds in their own account, never in the withholding account. Realized gains come from `realized_gains_inner`; deductible expenses are the EXPENSE splits of transactions carrying the book setting `tax_deductible_tag`. Tax roles guard account deletion and the orphan sweep like dividend links; `export_tax_summary_csv` reuses `fetch_tax_summary_inner`.
- The balance sheet (`/reports/balance-sheet?asOf=&compare=month|year|yoy`, `fetch_balance_sheet`) values the account tree through a date with `accounts::accounts_as_of`: splits up to that day (still capped by the visibility window), FX via `get_fx_map(as_of)` and prices via `PriceHistory::price_as_of`, the same rule as the net worth trend. No date means "now", identical to `fetch_accounts`. The comparison is a second valuation; rows pair by account GUID in `buildBalanceSheet`.
- `clear_app_db` (`settings.rs`) drops all tables and recreates `SCHEMA`, and bumps `book_generation` so in-flight price writes are discarded.

## Backend conventions

- Every DB-touching command runs through `db::run_blocking(state.db.clone(), move |conn| …)` — never lock the connection directly inside an `async fn`. Non-DB file I/O goes through `db::run_io(…)`; read import/CSV files before `run_blocking` so the DB lock is never held during disk I/O. Book switching follows the same rule via the mutable counterpart `db::run_swap_blocking`.
- Use `db::with_tx_err(conn, || …)` instead of manual `BEGIN`/`COMMIT`/`ROLLBACK` (RAII rollback on drop). Private cores follow `<command>_inner(conn, …)` and return `Result<T, db::AppError>` (`NotFound`/`Validation`/`Io`/`Db`/`Internal`); commands return `Result<T, AppError>` too and stay thin wrappers. `AppError` crosses IPC as `{ kind, message }` (`BackendError` in `tauri.ts`).
- Never match SQLite errors by message text — `DbError` carries the extended code (`is_foreign_key()`).
- Read helpers tolerate only `QueryReturnedNoRows` (`.optional()`); never swallow DB failures with `.ok()` or `unwrap_or_default()`.
- Visibility window: future-dated transactions are excluded from every "current state" query, details included, via `utils::VISIBLE_TX_FILTER` (or `utils::VISIBLE_NOW_EXPR` when the bound is an expression). The window is local-timezone (`date('now','localtime','+1 day')`), matching `todayStr()` and `chrono::Local`; timestamps stay UTC. Never hand-write the predicate or net worth/PnL/budgets diverge.
- Validate `YYYY-MM-DD` inputs with `utils::validate_iso_date` before SQL `date()` math — malformed values return NULL and silently zero comparisons.
- Recursive account-tree CTEs use `UNION` (not `UNION ALL`) so a corrupt `parent_guid` cycle terminates.
- Reuse `utils::RAW_AMOUNT`/`QUANTITY_AMOUNT`/`signed_amount_expr`, `sql::push_tx_date_search` and `sql::WhereBuilder` (auto-numbered `?N`, `param_offset` reserves leading indices) for dynamic WHERE clauses. Chunk `IN (…)` GUID lists with `sql::MAX_IN_PARAMS`, batch per-row lookups, and escape search input with `utils::escape_like` + `ESCAPE '\'`.
- Clamp client pagination at the boundary (`utils::clamp_page_size`/`clamp_page`) and compute totals with `utils::total_pages`.

## Frontend conventions

- Row-click navigation goes through `$lib/utils/navigation.ts` (`handleRowClick(e, go)` + `openTransaction`/`openAccount`/`openHolding`/`openBudget`); keep real `<a>` elements for keyboard and middle-click.
- Sidebar groups, labels, icons, chords and palette `Go to` commands come from `$lib/features/nav/nav-items.ts`; badges come from `$lib/stores/nav.svelte.ts` (`refreshNav` runs from the layout effect). Add pages by extending `NAV_GROUPS`, not the sidebar template.
- Reports link to each other instead of repeating content: Overview KPI cards open their report for the same period (`KpiCard` `href`), and Portfolio's Dividends card opens Realized gains.
- Page titles and descriptions live once in `$lib/features/nav/page-meta.ts` (`PAGE_META`) and feed `PageHeader`, `BookGate`, `<title>`, breadcrumbs and the sidebar. Copy is sentence case, plain and short (warmth only in empty states); descriptions carry no trailing period.
- `/` is home: the Overview with a book open, the welcome screen without one. Detail and form pages show `PageHeader` `breadcrumbs` instead of back buttons; secondary header actions go in `OverflowMenu`, keeping one primary action visible.
- Page roots use the `page-stack` utility (`page-form` narrows create/edit pages); density only swaps `--page-padding`/`--page-gap`. Page grids use container-query variants (`@xl:`, `@3xl:`…) against the `@container` main column, not viewport breakpoints; dialogs keep viewport variants.
- Every book switch (open/new/save-as/close, copy path) goes through `$lib/features/book/book-actions.ts`, and book-scoped transfers (GnuCash import, CSV wizard link, full-book CSV exports) through `$lib/features/book/import-export-actions.ts`; never re-implement these dialogs or session updates. The GnuCash confirm dialog is store-driven (`gnucash-import-dialog-state.svelte.ts` + `gnucash-import-dialog.svelte`, mounted in the layout).
- Table sort state uses `useTableSort<K>(defaults, descFirst)` from `$lib/hooks/`.
- URL state composes factories from `use-url-params.svelte.ts` (`pageParam`, `debouncedSearchParam`, `dateParam`, `yearParam`); free-form specs only when none fits.
- Search inputs debounce through `useDebouncedSearch`.
- Command failures reject with `BackendError` (`{ kind, message }`): show text via `errMsg(e, fallback)` and branch on `errKind(e)` / `useAsyncData().errorKind` (e.g. `'notFound'` → `EmptyState`), never on message text.
- Money values render through `<Amount>` (`$lib/components/shared/amount.svelte`: tabular digits, no wrap, -0 clamped, optional `tone`); amount table cells are just `text-right` around it.
- Hints use the `tooltip` attachment (`$lib/attachments/tooltip.ts`, `{@attach tooltip('…')}`; `{ truncated: true }` for cut-off text), never native `title`.
- Dates parse through `parseLocalDate` (date-only strings are local, not UTC); counts use `plural(n, 'word')`; ranges use `formatDateRange`.
- Dialog form reset uses `useDialogReset(() => open, sync)` (bits-ui never fires `onOpenChange` for programmatic `bind:open`); create/edit footers use `<SaveFooter>`.
- `index.ts` barrels use named exports only.
- `tauri.ts`: leaf payload types are module-private; export only what other modules import.
- localStorage keys are stable and unversioned (e.g. `spendo-settings`, `cols:budgets`) — no `:v2` suffixes or legacy branches; drop unknown or invalid stored values instead. A stored selection always wins: new default columns appear only with no stored state or after "Reset to default".
- The transaction form stages attachments picked before a create save in `pendingAttachments` and attaches them with `addAttachments` right after `createTransaction`; a failed attach leaves the transaction saved. Extension/count rules live in `$lib/features/transactions/attachment-utils.ts`, mirrored from `attachments.rs`.

## Naming glossary (do not invent synonyms)

- `guid: string` = stable entity id (accounts, transactions, commodities). `id: number` is only the autoincrement config row key (`import_rules.id`).
- `commodity` = DB row (security/currency), `holding` = aggregated position for one commodity, `symbol` = Yahoo ticker override, `mnemonic` = display code. Route `portfolio/[holding]` holds a `commodityGuid`.
- IPC args are single `camelCase` keys in TS mapping to `snake_case` in Rust via `#[tauri::command]`; prefer one `Input` struct with `rename_all = "camelCase"` for new commands.
