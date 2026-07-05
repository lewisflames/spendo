# spendo-app

SvelteKit + Svelte 5 (runes mode). Tailwind v4, adapter-node.

App has its own persistent SQLite DB (`data/spendo.db`). GnuCash `.gnucash` files are imported/merged into it via `INSERT OR IGNORE` (GUID dedup). No ephemeral sessions — data persists across reloads.

## Commands

| Action     | Command                                        |
| ---------- | ---------------------------------------------- |
| Dev server | `bun run dev`                                  |
| Build      | `bun run build`                                |
| Preview    | `bun run preview`                              |
| Type-check | `bun run check` (runs `svelte-kit sync` first) |
| Lint       | `bun run lint` (prettier --check + eslint)     |
| Format     | `bun run format` (prettier --write)            |

No test framework configured.

## Code style

- **Svelte 5 runes**: `$props()`, `$derived()`, `$state()` — no `export let` or `on:click`; use `onclick`, `{@render children()}`
- **Prettier**: tabs, single quotes, no trailing commas, print width 100. Tailwind class sorting via `prettier-plugin-tailwindcss`
- **Tailwind v4**: `@import 'tailwindcss'` (Vite plugin, no PostCSS)
- **TypeScript**: strict mode, `rewriteRelativeImportExtensions` enabled, `moduleResolution: "bundler"`
- **Icons**: Lucide (`@lucide/svelte`)
- **`compilerOptions.experimental.async: true`** — top-level `await` allowed in components

## Architecture

- **No `+server.ts` / `+page.server.ts`**. All server functions via SvelteKit `remoteFunctions`: `form` (file upload in `$lib/db.remote.ts`) and `query` (RPC reads in `$lib/*.remote.ts`).
- **Server-only code** in `$lib/server/`. Each function opens `better-sqlite3` via `getAppDbPath()` (not a token), queries, closes in `finally`.
- **App DB**: `data/spendo.db` (configurable via `DATA_DIR` env var). Schema created on first use. Tables mirror GnuCash columns for `commodities`, `accounts`, `transactions`, `splits`, `prices` plus `import_log` for tracking imports.
- **Price cache**: in-memory map keyed by fixed `'app'` key (not token-based). Cleared on DB reset.
- **Routes**: `/` (import or overview), `/accounts`, `/accounts/[guid]`, `/dashboard`, `/transactions`, `/prices`, `/settings`, `/about`, `/license`. All SPA — no SSR data fetching.

## Data / imports

- Accepts **only `.gnucash` files** (GnuCash SQLite format). Max 100 MB.
- Upload via `UploadDialog` component or inline "Import" button on homepage.
- `importGnucash()` in `$lib/server/import-gnucash.ts` reads source file, merges into app DB. Uploaded temp files deleted after import.
- `clearAppDb()` in `$lib/server/app-db.ts` deletes all rows (keeps schema). Exposed via `clearAppDb` remote in Settings.
- Homepage fetches `fetchHomeData` (`db.remote.ts`) on mount to restore session across reloads. Displays last import filename/date + account/transaction counts.
- `*.db` and `*.gnucash` gitignored.

## shadcn-svelte (Nova)

- Components under `$lib/components/ui/`. Style is `nova` (see `components.json`).
- Uses `tailwind-variants` (`tv`), `bits-ui` for headless primitives.
- `$lib/utils.ts`: `cn()` (clsx + tailwind-merge).
- CSS variables in `src/routes/layout.css` (not `src/app.css`).

## Svelte MCP

Wired in `.opencode/opencode.json` (plugin `@sveltejs/opencode`). Use for docs lookups and code validation.

## Build artifacts

- Output goes to `/build`
- Generated `.svelte-kit/` is gitignored (regenerated via `bun run prepare` / `svelte-kit sync`)

## Desktop chrome

`app-region: drag` title bar with min/max/close buttons — front-end only, no Tauri/Electron.
