# spendo

SvelteKit + Svelte 5 (runes mode). Tailwind v4. Tauri v2 desktop app.

GnuCash financial data importer/explorer. Rust backend (`rusqlite`) + static SvelteKit frontend (`@sveltejs/adapter-static`).

## Commands

| Action      | Command                                                                                  |
| ----------- | ---------------------------------------------------------------------------------------- |
| Dev server  | `bun run dev` (`vite dev`)                                                               |
| Build       | `bun run build` (`vite build`)                                                           |
| Prepare     | `bun run prepare` (svelte-kit sync)                                                      |
| Type-check  | `bun run check` (runs `svelte-kit sync` first)                                           |
| Lint        | `bun run lint` (`prettier --check . && eslint .`)                                        |
| Format      | `bun run format` (`prettier --write .`)                                                  |
| Tauri dev   | `bun run tauri:dev` (runs `generate-icons` first)                                        |
| Tauri build | `bun run tauri:build` (runs `generate-icons` first, `tauri build --features production`) |

No `preview` script (no `vite preview` wrapper). No frontend test framework. Rust unit tests (colocated in `src-tauri/src/` modules) run with `cargo test` (in `src-tauri/`); frontend is verified via `bun run check` + `bun run lint`.

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
- **Rust modules** (`src-tauri/src/`): `db`, `accounts`, `account_detail`, `transactions`, `prices`, `import`, `home`, `reports`, `budgets`, `portfolio`, `settings`, `utils`, plus support modules `fx`, `sql`, `tree`, `yahoo`. Cargo crate: `spendo_app_lib`.
- **DB**: SQLite via `rusqlite`. Connection held as `Arc<Mutex<Connection>>` in Tauri state (`AppState` also carries `db_path`), initialized once at startup. DB file lives in Tauri's `app_data_dir` (`spendo.db`). Schema created on first use in `db::init_db`. No migrations by design (pre-release): `SCHEMA` is the single source of truth; schema changes require a database reset (Settings → Clear all data, or delete `spendo.db`) followed by re-import.
- **Routes**: `/`, `/accounts`, `/accounts/[guid]`, `/budgets`, `/budgets/[accountGuid]`, `/reports/pnl`, `/portfolio`, `/portfolio/[holding]`, `/transactions`, `/transactions/[id]`, `/settings`, `/about` (plus dev-only `/dev/force-test`). `/accounts` is the single balance-sheet view (as-of picker + accounting identity); `/reports` holds only period reports (P&L).
- **Frontend layout**: routes contain only route files (`+page.svelte` etc.). Page-specific components/types live in `$lib/features/<domain>/` (`home`, `accounts`, `budgets`, `pnl`, `reports`, `portfolio`, `transactions`). Domain utilities in `$lib/utils/` (`csv`, `export`, `error`, `format.svelte.ts`, `date-utils.svelte.ts`, plus `navigation.ts`, `reconcile.ts`).

## Data / imports

- Accepts **only `.gnucash` files** (GnuCash SQLite format). Users pick files via Tauri file dialog (`@tauri-apps/plugin-dialog`).
- Import in `src-tauri/src/import.rs`: reads source DB tables (commodities, accounts, transactions, splits — `prices` are NOT imported from the source book), merges into app DB via GUID-keyed upserts (`ON CONFLICT … DO UPDATE`) plus an orphan sweep (rows absent from the source are deleted); source file is opened strictly read-only.
- `clear_app_db` in `settings.rs` performs a full reset (drops all tables, including legacy ones, then recreates `SCHEMA`). Frontend calls via `clearAppDb()` in `$lib/tauri.ts`. Exposed on Settings page.
- Price data: live prices fetched from Yahoo only (`yahoo.rs` + `fx.rs`) via `reqwest` + async tokio runtime; never imported from the source book. Per-commodity Yahoo symbol overrides live in the app-local `symbol_overrides` table (`set/delete_symbol_override` in `prices.rs`, edited on the holding detail page). Latest price per commodity queried from DB via `get_cached_prices_map()`.

## shadcn-svelte (Nova)

- Components under `$lib/components/ui/` (shadcn CLI-managed; alias in `components.json`). Hand-written components in `$lib/components/layout/` (app chrome) and `$lib/components/shared/` (reusable app-wide).
- Uses `tailwind-variants` (`tv`), `bits-ui` for headless primitives.
- `$lib/utils.ts`: `cn()` (clsx + tailwind-merge). Domain utilities live in `$lib/utils/`.
- CSS variables in `src/app.css`.

## Notables

- **Frameless window**: `decorations: false` in Tauri config. Custom `<TitleBar />` component handles drag.
- **Theme toggle**: `Ctrl/Cmd+Shift+T` toggles light/dark. Persisted in `localStorage`, validated per-key on load (`settings.svelte.ts`).
- **Store convention**: `.svelte.ts` modules using `$state()` wrapped in `{ value }` (e.g., `export const x = $state({ value: ... })`). Applies to every store, including scalar flags like `keyboardShortcutsDialog`.
- **Sample data**: `data/test.gnucash` exists but is gitignored (`/data` in `.gitignore`).
- **Charting/date deps**: `layerchart` (charts), `tw-animate-css` (animations), `@internationalized/date` (date utils).
- **Build output**: `/build`, `.svelte-kit/` gitignored (regenerated via `bun run prepare`).
- **Svelte MCP**: Wired via `@sveltejs/opencode` plugin in `.opencode/opencode.json`.

## Backend conventions

- **Every DB-touching command** runs through `db::run_blocking(state.db.clone(), move |conn| …)` — never lock the connection directly inside an `async fn` (it blocks a tokio worker). File I/O (hashing, dialogs) happens before acquiring the lock.
- **Visibility window**: future-dated transactions are excluded from every "current state" query via `utils::VISIBLE_TX_FILTER`. Don't hand-write the predicate — new money queries must use it or net worth/PnL/budgets diverge.
- **SQL fragments**: reuse `utils::RAW_AMOUNT` / `QUANTITY_AMOUNT` / `signed_amount_expr` and the `sql::WhereBuilder` for dynamic WHERE clauses (auto-numbered `?N`, `param_offset` reserves leading indices). Escape user search input with `utils::escape_like` + `ESCAPE '\'`.
- **Pagination**: clamp client input at the boundary with `utils::clamp_page_size` / `clamp_page`; compute totals with `utils::total_pages`.
- Private command cores follow `<command>_inner(conn, …)` naming; commands stay thin wrappers.

## Frontend conventions

- **Navigation**: row-click navigation goes through `$lib/utils/navigation.ts` (`handleRowClick(e, go)` + `isPlainRowClick`, `openTransaction`, `openAccount`, `gotoHolding`, `openBudget`) — never copy per-table click guards; keep real `<a>` elements for keyboard/middle-click.
- **Sorting**: table sort state uses `useTableSort<K>(defaults, descFirst)` from `$lib/hooks/`.
- **URL state**: compose specs from the factories in `use-url-params.svelte.ts` (`pageParam`, `debouncedSearchParam`, `dateParam`, `yearParam`); free-form specs only when no factory fits.
- **Search inputs**: always debounce through `useDebouncedSearch`.
- **Formatting**: dates parse through `parseLocalDate` (date-only strings are local, not UTC); counts use `plural(n, 'word')`; ranges use `formatDateRange`. Route titles come from `$lib/routes.ts` (`routeTitle`).
- **Barrels**: named exports only in `index.ts` re-export files (no default+named duplication).
- **tauri.ts**: leaf payload types are module-private; only export types that other modules import.
