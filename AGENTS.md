# spendo

SvelteKit + Svelte 5 (runes mode). Tailwind v4. Tauri v2 desktop app.

Local-first personal finance manager (portable `.spendo` books). Rust backend (`rusqlite`) + static SvelteKit frontend (`@sveltejs/adapter-static`). GnuCash is one import source, not the app's center.

## Commands

| Action          | Command                                                                                  |
| --------------- | ---------------------------------------------------------------------------------------- |
| Dev server      | `bun run dev` (`vite dev`)                                                               |
| Build           | `bun run build` (`vite build`)                                                           |
| Prepare         | `bun run prepare` (svelte-kit sync)                                                      |
| Type-check      | `bun run check` (runs `svelte-kit sync` first)                                           |
| Test (frontend) | `bun run test` (`vitest run`, colocated `src/**/*.test.ts`)                              |
| Lint            | `bun run lint` (`prettier --check . && eslint .`)                                        |
| Format          | `bun run format` (`prettier --write .`)                                                  |
| Tauri dev       | `bun run tauri:dev` (runs `generate-icons` first)                                        |
| Tauri build     | `bun run tauri:build` (runs `generate-icons` first, `tauri build --features production`) |

No `preview` script (no `vite preview` wrapper). Frontend unit tests are pure-helper `vitest` suites colocated as `src/**/*.test.ts` (`bun run test`); Rust unit tests (colocated in `src-tauri/src/` modules) run with `cargo test` (in `src-tauri/`); frontend is verified via `bun run check` + `bun run lint` + `bun run test`.

## Code style

- **Svelte 5 runes**: `$props()`, `$derived()`, `$state()` — no `export let` or `on:click`; use `onclick`, `{@render children()}`
- **Prettier**: 2-space indent (Prettier default; no `useTabs` in `.prettierrc`), single quotes, no trailing commas, print width 100. Tailwind class sorting via `prettier-plugin-tailwindcss`
- **Tailwind v4**: `@import 'tailwindcss'` (Vite plugin, no PostCSS). Stylesheet at `src/app.css`.
- **TypeScript**: strict mode, `rewriteRelativeImportExtensions` enabled, `moduleResolution: "bundler"`
- **`compilerOptions.experimental.async: true` in `svelte.config.js`** — top-level `await` allowed in components
- **Icons**: Lucide (`@lucide/svelte`)
- **Rust backend**: `serde` with `#[serde(rename_all = "camelCase")]` on all serialized response structs

## Architecture

- **Tauri v2** desktop app. Frontend is static SvelteKit (adapter-static, SPA, no SSR). Backend is Rust in `src-tauri/src/`.
- **All backend calls** go through `@tauri-apps/api/core` `invoke()` via `$lib/tauri.ts`. No `+server.ts` / `+page.server.ts`.
- **Rust modules** (`src-tauri/src/`): `db`, `accounts`, `accounts_mut`, `account_detail`, `transactions`, `attachments`, `prices`, `import`, `import_rules`, `csv_import`, `home`, `reports`, `budgets`, `portfolio`, `settings`, `books`, `export`, `history`, `reconcile`, `schedules`, `search`, `init`, `nav`, `utils`, plus support modules `fx`, `sql`, `tree`, `yahoo`. Cargo crate: `spendo_app_lib`. `reports` owns only P&L (`/accounts` is the balance-sheet view).
- **DB**: SQLite via `rusqlite`. Document model: one active `.spendo` book opened in place; connection held as `Arc<Mutex<Connection>>` in Tauri state (`AppState` also carries `book_generation: Arc<AtomicU64>` — bumped on every connection swap while the DB lock is held — `db_path: Mutex<Option<PathBuf>>` — `None` while book-less — plus `app_data_dir`). Long-running network writes (price refresh, import) capture the generation before fetching and abort if it changed. Book switching (`books.rs`: `open_book`/`create_book_at`/`save_book_as`/`close_book`) replaces the `Connection` via `db::run_swap_blocking` (the mutable counterpart of `run_blocking`) and remembers the path in `app_data_dir/last_book.json`; `close_book` swaps in the `db::open_placeholder` in-memory book and clears the last-book sidecar. Books are stamped with `PRAGMA application_id` (`SPENDO_APP_ID`) so Open can reject GnuCash sources / random SQLite. Schema created on first use in `db::init_db`. New books are seeded with the internal ROOT plus the five canonical category roots (Assets/Liabilities/Equity/Income/Expenses, placeholders); `init::ensure_default_roots` heals books missing them on open, and GnuCash import protects the roots of categories the source lacks from its orphan sweep, and refuses to run against a source with no accounts (an empty source set would sweep the whole book); accounts carrying budgets, referenced by import rules, or referenced by a schedule template are protected from the sweep too (they cascade on account delete or would leave dangling template references, and are outside undo snapshots). No migrations by design (pre-release): `SCHEMA` is the single source of truth; schema changes require a database reset (Settings → Clear all data, or delete the book file) followed by re-import. `csv_import_presets` (saved CSV mappings keyed by header signature) and `import_rules` (description → category rules) are book-scoped config outside `DATA_TABLES`/undo. Transaction receipts live as BLOBs in the `attachments` table (inside the book, so Save As carries them); they are deliberately outside `DATA_TABLES`/undo and are deleted permanently via FK cascade when their transaction goes.
- **Routes**: `/` (welcome-only; redirects to `/transactions` once a book is open), `/accounts`, `/accounts/[guid]`, `/budgets`, `/budgets/[accountGuid]`, `/reports/pnl`, `/portfolio`, `/portfolio/[holding]`, `/transactions`, `/transactions/new`, `/transactions/[guid]`, `/transactions/[guid]/edit`, `/scheduled`, `/scheduled/new`, `/scheduled/[guid]/edit`, `/reconcile`, `/reconcile/[guid]`, `/history`, `/settings`, `/import/csv`, `/about`. `/accounts` is the single balance-sheet view (accounting identity); `/reports` holds only period reports (P&L); `/import/csv` hosts the CSV wizard (GnuCash import and CSV exports are menu/palette actions, not pages).
- **Frontend layout**: routes contain only route files (`+page.svelte` etc.). Page-specific components/types live in `$lib/features/<domain>/` (`accounts`, `book`, `budgets`, `csv-import`, `history`, `nav`, `palette`, `pnl`, `portfolio`, `reconcile`, `reports`, `schedules`, `settings`, `transactions`, `welcome`). Domain utilities in `$lib/utils/` (`csv`, `error`, `format.svelte.ts`, `date-utils.svelte.ts`, plus `navigation.ts`, `reconcile.ts`).
- **Browser dev mock**: `bun run dev` opened in a plain browser (no Tauri runtime) routes every `invoke()` through `$lib/dev/mock-invoke.ts`, a stateful fixture backend over the synthetic book in `$lib/dev/fixtures.ts`. Handlers are typed against `MockPayloads` in `$lib/tauri.ts`; unknown commands throw with a pointer. `import.meta.env.DEV` gates the dynamic import so fixtures never ship in production. File dialogs and OS-level actions still require `tauri:dev`.

## Data / imports

- GnuCash import accepts **only `.gnucash` files** (GnuCash SQLite format) and requires an open book first — it merges _into_ the active `.spendo` book, never opens in place. Users pick files via Tauri file dialog (`@tauri-apps/plugin-dialog`). The flow is always preview → confirm → merge: `preview_gnucash_import` hashes the file and returns exact add/update/delete counts without writing, `import_gnucash` re-hashes (stale files are rejected) and runs with the user's `deleteOrphans` choice. Re-running a past import is available from its history entry.
- GnuCash scheduled transactions (`schedxactions`/`recurrences` + the KVP templates under the book's template root) convert into `recurring_schedules` keyed by the SX GUID, in the same preview/merge; the template accounts, template transactions and their zero-value splits are excluded from the account/transaction/split merge (they are GnuCash plumbing). Schedules are never swept: accounts they reference are added to the sweep's protected set. Re-import refreshes the definition but keeps local `active` and `occurrences_posted`, and only moves `next_due_date` forward (the source's generated instances are imported as ordinary transactions, so the pointer starts after them). Recurrence rules Spendo can't express (multiple rules, `once`, end-of-month, nth/last weekday, weekend adjustment is ignored) are reported as `skippedSchedules` with reasons. Import-added schedules are snapshotted into the import's history entry, so undo removes/restores them; manual schedule CRUD still writes no `change_log` entries.
- CSV import (`csv_import.rs`, wizard in `$lib/features/csv-import/` on `/import/csv`) is a detect → map → preview → commit flow for bank statements (`.csv`/`.txt`/`.tsv`); it also merges into the active book. `csv_detect` returns a SHA-256 that preview/commit echo back (a changed file aborts instead of importing rows the user never saw). Successful commits remember the mapping per header signature (`csv_import_presets`), applied automatically on the next statement with the same layout; `rememberMapping: false` skips it and "Forget" deletes it.
- Import rules (`import_rules.rs`, Settings → Import rules): the first rule (priority order) whose pattern `contains`/`equals` a row's description picks that row's category; the mapping step's offset account is the fallback. Managed with CRUD commands and applied inside `parse_file_rows`, so the preview's Category column and the commit agree.
- Import in `src-tauri/src/import.rs`: reads source DB tables (commodities, accounts, transactions, splits — `prices` are NOT imported from the source book), merges into app DB via GUID-keyed upserts (`ON CONFLICT … DO UPDATE`) plus an orphan sweep (rows absent from the source are deleted unless `deleteOrphans: false`); source file is opened strictly read-only.
- CSV exports (`export.rs`) are generated and written in Rust in one command each (`export_transactions_csv`, `export_accounts_csv`) — no full-book row payload crosses IPC. Transactions export is a complete dump: it deliberately ignores the visibility window so future-dated rows survive; a CSV round trip back into Spendo flags those as errors. Locale date rendering comes from `exportDatePattern()` (`$lib/features/book/exporters.ts`), which probes `Intl.DateTimeFormat` and passes part order + separator.
- `clear_app_db` in `settings.rs` performs a full reset (drops all tables, then recreates `SCHEMA`). Frontend calls via `clearAppDb()` in `$lib/tauri.ts`. Exposed on Settings page.
- Price data: live prices fetched from Yahoo only (`yahoo.rs` + `fx.rs`) via `reqwest` + async tokio runtime; never imported from the source book. Per-commodity Yahoo symbol overrides live in the app-local `symbol_overrides` table (`set/delete_symbol_override` in `prices.rs`, edited on the holding detail page). Latest price per commodity queried from DB via `get_cached_prices_map()`.

## shadcn-svelte (Nova)

- Components under `$lib/components/ui/` (shadcn CLI-managed; alias in `components.json`). Hand-written components in `$lib/components/layout/` (app chrome) and `$lib/components/shared/` (reusable app-wide).
- Uses `tailwind-variants` (`tv`), `bits-ui` for headless primitives.
- `$lib/utils.ts`: `cn()` (clsx + tailwind-merge). Domain utilities live in `$lib/utils/`.
- CSS variables in `src/app.css`.

## Notables

- **Frameless window**: `decorations: false` in Tauri config. Custom `<TitleBar />` component handles drag; drag/maximize handlers come from `useWindowDrag()` and modifier-key labels from `$lib/utils/platform.ts`.
- **Theme toggle**: `Ctrl/Cmd+Shift+T` toggles light/dark. Persisted in `localStorage`, validated per-key on load (`settings.svelte.ts`).
- **Store convention**: `.svelte.ts` modules using `$state()` wrapped in `{ value }` (e.g., `export const x = $state({ value: ... })`). Applies to every store, including scalar flags like `keyboardShortcutsDialog`.
- **Sample data**: `data/test.gnucash` exists but is gitignored (`/data` in `.gitignore`).
- **Charting/date deps**: `layerchart` (charts), `tw-animate-css` (animations), `@internationalized/date` (date utils).
- **Build output**: `/build`, `.svelte-kit/` gitignored (regenerated via `bun run prepare`).
- **Svelte MCP**: Wired via `@sveltejs/opencode` plugin in `.opencode/opencode.json`.

## Backend conventions

- **Every DB-touching command** runs through `db::run_blocking(state.db.clone(), move |conn| …)` — never lock the connection directly inside an `async fn` (it blocks a tokio worker). Non-DB file I/O in an async command goes through `db::run_io(…)` (spawn_blocking without the connection lock), and import/CSV files are read **before** `run_blocking` so the DB lock is never held during disk I/O. Book switching uses the mutable counterpart `db::run_swap_blocking` (see `books.rs`).
- **Transactions**: use `db::with_tx_err(conn, || …)` instead of manual `BEGIN`/`COMMIT`/`ROLLBACK` (RAII rollback on drop). `*_inner` functions return `Result<T, db::AppError>` (`NotFound`/`Validation`/`Db`) and convert to `String` only at the `#[tauri::command]` boundary (`From<AppError>` keeps messages identical). Never match SQLite errors by message text — `DbError` carries the extended code (`is_foreign_key()`).
- **Error propagation**: read helpers tolerate only `QueryReturnedNoRows` (`.optional()`); never swallow arbitrary DB failures with `.ok()` or `unwrap_or_default()` — corruption must surface, not silently degrade to empty currency/price data.
- **Visibility window**: future-dated transactions are excluded from every "current state" query via `utils::VISIBLE_TX_FILTER` (detail queries included). The window is evaluated in the user's local timezone (`date('now','localtime','+1 day')`), matching the frontend's `todayStr()` and the backend's `chrono::Local` "today" checks; timestamps (`iso_now`) stay UTC. Don't hand-write the predicate — new money queries must use it (or `utils::VISIBLE_NOW_EXPR` when the bound is needed as an expression) or net worth/PnL/budgets diverge.
- **Date inputs**: validate `YYYY-MM-DD` strings with `utils::validate_iso_date` before interpolating them into SQL `date()` math — malformed values make SQLite return NULL and silently zero out comparisons.
- **Recursive CTEs**: use `UNION` (not `UNION ALL`) when walking account trees so a corrupt `parent_guid` cycle terminates.
- **SQL fragments**: reuse `utils::RAW_AMOUNT` / `QUANTITY_AMOUNT` / `signed_amount_expr`, the shared date/search predicate `sql::push_tx_date_search`, and the `sql::WhereBuilder` for dynamic WHERE clauses (auto-numbered `?N`, `param_offset` reserves leading indices). Chunk `IN (…)` GUID lists with `sql::MAX_IN_PARAMS`; batch per-row lookups instead of querying in a Rust loop. Escape user search input with `utils::escape_like` + `ESCAPE '\'`.
- **Pagination**: clamp client input at the boundary with `utils::clamp_page_size` / `clamp_page`; compute totals with `utils::total_pages`.
- Private command cores follow `<command>_inner(conn, …)` naming; commands stay thin wrappers.

## Frontend conventions

- **Navigation**: row-click navigation goes through `$lib/utils/navigation.ts` (`handleRowClick(e, go)` + `openTransaction`, `openAccount`, `openHolding`, `openBudget`) — never copy per-table click guards; keep real `<a>` elements for keyboard/middle-click.
- **Sidebar / palette nav**: sidebar groups, labels, icons, chords and palette `Go to` commands come from `$lib/features/nav/nav-items.ts`; account-root targets resolve at runtime from `$lib/stores/nav.svelte.ts` (`refreshNav` runs from the layout effect). Add a page by extending `NAV_GROUPS`, not the sidebar template.
- **Book actions**: every book switch (open/new/save-as/close, copy path) goes through `$lib/features/book/book-actions.ts` — the sidebar book menu, Settings and the command palette all call these; never re-implement the dialogs or session-store updates. Book-scoped transfers (GnuCash import, CSV wizard link, full-book CSV exports) follow the same rule via `$lib/features/book/import-export-actions.ts`; the GnuCash confirm dialog is store-driven (`gnucash-import-dialog-state.svelte.ts` + `gnucash-import-dialog.svelte`, mounted in the layout), and Settings holds the CSV format preferences plus the import-rules card.
- **Sorting**: table sort state uses `useTableSort<K>(defaults, descFirst)` from `$lib/hooks/`.
- **URL state**: compose specs from the factories in `use-url-params.svelte.ts` (`pageParam`, `debouncedSearchParam`, `dateParam`, `yearParam`); free-form specs only when no factory fits.
- **Search inputs**: always debounce through `useDebouncedSearch`.
- **Formatting**: dates parse through `parseLocalDate` (date-only strings are local, not UTC); counts use `plural(n, 'word')`; ranges use `formatDateRange`. Route titles come from `$lib/routes.ts` (`routeTitle`).
- **Dialogs**: form reset goes through `useDialogReset(() => open, sync)` (bits-ui never fires `onOpenChange` for programmatic `bind:open`); create/edit footers use `<SaveFooter layout=…>` — never copy the reset effect or footer buttons.
- **Barrels**: named exports only in `index.ts` re-export files (no default+named duplication).
- **tauri.ts**: leaf payload types are module-private; only export types that other modules import.
- **Persisted frontend state**: localStorage keys are stable and unversioned (e.g. `spendo-settings`, `cols:budgets`). Never add version suffixes (`:v2`) or migration/legacy-compat branches; drop unknown or invalid stored values instead. A stored selection always wins — new default columns appear only for users with no stored state or after "Reset to default" in the Columns menu.

## Naming glossary (do not invent synonyms)

- `guid: string` = stable entity id (accounts, transactions, commodities). `id: number` is only the numeric history row (`ChangeEntry.id`, `revert_history_entry(id)`); route `transactions/[guid]` holds a GUID.
- `commodity` = DB row (security/currency), `holding` = aggregated position for one commodity, `symbol` = Yahoo ticker override, `mnemonic` = display code. Route `portfolio/[holding]` holds a `commodityGuid`.
- IPC args are single `camelCase` keys in TS mapping to `snake_case` in Rust via `#[tauri::command]`; prefer one `Input` struct with `rename_all = "camelCase"` for new commands.
