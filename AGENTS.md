# spendo

Tauri v2 desktop app: static SvelteKit SPA (Svelte 5 runes, Tailwind v4, `@sveltejs/adapter-static`, no SSR) over a Rust (`rusqlite`) backend. Local-first personal finance manager with portable `.spendo` books; GnuCash is one import source, not the app's center.

## Commands

| Action          | Command                                               |
| --------------- | ----------------------------------------------------- |
| Dev server      | `bun run dev`                                         |
| Build           | `bun run build`                                       |
| Type-check      | `bun run check`                                       |
| Test (frontend) | `bun run test` (vitest; colocated `src/**/*.test.ts`) |
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
- In a plain browser, `invoke()` routes through the DEV-gated `$lib/dev/mock-invoke.ts` over `$lib/dev/fixtures.ts`; file dialogs and OS actions still need `tauri:dev`.

## Data / imports

- GnuCash import accepts only `.gnucash` (SQLite) files and merges into the active book — never opens in place. Always preview → confirm → merge: the preview hashes the file and reports add/update/delete counts without writing; the import re-hashes and rejects a stale file. The source opens strictly read-only, and `prices` are never imported from it.
- The orphan sweep deletes rows absent from the source unless `deleteOrphans: false`. It protects the roots of categories the source lacks and accounts carrying budgets or referenced by import rules/schedules; schedules themselves are never swept. Re-import refreshes a schedule definition but keeps local `active`/`occurrences_posted` and only moves `next_due_date` forward. A source with no accounts is rejected.
- CSV import (`/import/csv`) is detect → map → preview → commit for `.csv`/`.txt`/`.tsv`; the detect SHA-256 is echoed through preview/commit, so a changed file aborts. Commits remember the mapping per header signature (`csv_import_presets`) unless `rememberMapping: false`.
- Import rules pick the category for the first rule (priority order) whose pattern `contains`/`equals` the row's description; the mapping step's offset account is the fallback. Rules apply inside `parse_file_rows`, so preview and commit agree.
- CSV exports are written in Rust in one command each (`export_transactions_csv`, `export_accounts_csv`) — no full-book payload crosses IPC. The transactions export deliberately ignores the visibility window so future-dated rows survive, and a round trip back flags them as errors.
- GnuCash export (`export_gnucash`, `gnucash_export.rs`) writes a real GnuCash SQLite book: the 5.16 DDL and `versions` rows verbatim, canonical hyphenless GUIDs (GnuCash matches some columns by exact string), accounts/transactions/splits with notes slots, prices, budgets (declaring the natural-sign feature so GnuCash skips its sign-migration notice) and schedules as template accounts with `sched-xaction` KVP frames. Currencies keep GnuCash's 2-decimal smallest unit; security fractions come from the share denominators. Receipts/import rules/CSV presets aren't exportable, and splits finer than a commodity's smallest unit (which GnuCash rounds on load) are counted in the result. Target a new GnuCash release by replacing the DDL and per-table versions together, then re-verifying against a book that release wrote.
- Prices come from Yahoo only (`yahoo.rs` + `fx.rs`), never from the source book; per-commodity overrides live in `symbol_overrides`.
- `clear_app_db` (`settings.rs`) drops all tables and recreates `SCHEMA`.

## Backend conventions

- Every DB-touching command runs through `db::run_blocking(state.db.clone(), move |conn| …)` — never lock the connection directly inside an `async fn`. Non-DB file I/O goes through `db::run_io(…)`; read import/CSV files before `run_blocking` so the DB lock is never held during disk I/O. Book switching follows the same rule via the mutable counterpart `db::run_swap_blocking`.
- Use `db::with_tx_err(conn, || …)` instead of manual `BEGIN`/`COMMIT`/`ROLLBACK` (RAII rollback on drop). Private cores follow `<command>_inner(conn, …)`, return `Result<T, db::AppError>` (`NotFound`/`Validation`/`Db`) and convert to `String` only at the `#[tauri::command]` boundary; commands stay thin wrappers.
- Never match SQLite errors by message text — `DbError` carries the extended code (`is_foreign_key()`).
- Read helpers tolerate only `QueryReturnedNoRows` (`.optional()`); never swallow DB failures with `.ok()` or `unwrap_or_default()`.
- Visibility window: future-dated transactions are excluded from every "current state" query, details included, via `utils::VISIBLE_TX_FILTER` (or `utils::VISIBLE_NOW_EXPR` when the bound is an expression). The window is local-timezone (`date('now','localtime','+1 day')`), matching `todayStr()` and `chrono::Local`; timestamps stay UTC. Never hand-write the predicate or net worth/PnL/budgets diverge.
- Validate `YYYY-MM-DD` inputs with `utils::validate_iso_date` before SQL `date()` math — malformed values return NULL and silently zero comparisons.
- Recursive account-tree CTEs use `UNION` (not `UNION ALL`) so a corrupt `parent_guid` cycle terminates.
- Reuse `utils::RAW_AMOUNT`/`QUANTITY_AMOUNT`/`signed_amount_expr`, `sql::push_tx_date_search` and `sql::WhereBuilder` (auto-numbered `?N`, `param_offset` reserves leading indices) for dynamic WHERE clauses. Chunk `IN (…)` GUID lists with `sql::MAX_IN_PARAMS`, batch per-row lookups, and escape search input with `utils::escape_like` + `ESCAPE '\'`.
- Clamp client pagination at the boundary (`utils::clamp_page_size`/`clamp_page`) and compute totals with `utils::total_pages`.

## Frontend conventions

- Row-click navigation goes through `$lib/utils/navigation.ts` (`handleRowClick(e, go)` + `openTransaction`/`openAccount`/`openHolding`/`openBudget`); keep real `<a>` elements for keyboard and middle-click.
- Sidebar groups, labels, icons, chords and palette `Go to` commands come from `$lib/features/nav/nav-items.ts`; account-root targets resolve from `$lib/stores/nav.svelte.ts` (`refreshNav` runs from the layout effect). Add pages by extending `NAV_GROUPS`, not the sidebar template.
- Every book switch (open/new/save-as/close, copy path) goes through `$lib/features/book/book-actions.ts`, and book-scoped transfers (GnuCash import, CSV wizard link, full-book CSV exports) through `$lib/features/book/import-export-actions.ts`; never re-implement these dialogs or session updates. The GnuCash confirm dialog is store-driven (`gnucash-import-dialog-state.svelte.ts` + `gnucash-import-dialog.svelte`, mounted in the layout).
- Table sort state uses `useTableSort<K>(defaults, descFirst)` from `$lib/hooks/`.
- URL state composes factories from `use-url-params.svelte.ts` (`pageParam`, `debouncedSearchParam`, `dateParam`, `yearParam`); free-form specs only when none fits.
- Search inputs debounce through `useDebouncedSearch`.
- Dates parse through `parseLocalDate` (date-only strings are local, not UTC); counts use `plural(n, 'word')`; ranges use `formatDateRange`.
- Dialog form reset uses `useDialogReset(() => open, sync)` (bits-ui never fires `onOpenChange` for programmatic `bind:open`); create/edit footers use `<SaveFooter>`.
- `index.ts` barrels use named exports only.
- `tauri.ts`: leaf payload types are module-private; export only what other modules import.
- localStorage keys are stable and unversioned (e.g. `spendo-settings`, `cols:budgets`) — no `:v2` suffixes or legacy branches; drop unknown or invalid stored values instead. A stored selection always wins: new default columns appear only with no stored state or after "Reset to default".

## Naming glossary (do not invent synonyms)

- `guid: string` = stable entity id (accounts, transactions, commodities). `id: number` is only the autoincrement config row key (`import_rules.id`).
- `commodity` = DB row (security/currency), `holding` = aggregated position for one commodity, `symbol` = Yahoo ticker override, `mnemonic` = display code. Route `portfolio/[holding]` holds a `commodityGuid`.
- IPC args are single `camelCase` keys in TS mapping to `snake_case` in Rust via `#[tauri::command]`; prefer one `Input` struct with `rename_all = "camelCase"` for new commands.
