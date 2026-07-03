# spendo-app

SvelteKit + Svelte 5 (runes mode). SQLite (GnuCash), Tailwind v4, adapter-node.

## Commands

| Action     | Command                                        |
| ---------- | ---------------------------------------------- |
| Dev server | `bun run dev`                                  |
| Build      | `bun run build`                                |
| Preview    | `bun run preview`                              |
| Type-check | `bun run check` (runs `svelte-kit sync` first) |
| Lint       | `bun run lint` (prettier --check + eslint)     |
| Format     | `bun run format` (prettier --write)            |
| Prepare    | `bun run prepare` (runs `svelte-kit sync`)     |

## Testing

No test framework configured. Do not attempt to run tests.

## Code style

- **Svelte 5 runes**: `$props()`, `$derived()`, `$state()` — no `export let` or `on:click`; use `onclick`, `{@render children()}`
- **Prettier**: tabs, single quotes, no trailing commas, print width 100. Tailwind class sorting via `prettier-plugin-tailwindcss` (stylesheet at `src/routes/layout.css`)
- **Tailwind v4**: `@import 'tailwindcss'` (no PostCSS, no `@tailwind` directives). Vite plugin (`@tailwindcss/vite`), not PostCSS.
- **TypeScript**: strict mode, `rewriteRelativeImportExtensions` enabled, `moduleResolution: "bundler"`
- **Icons**: Lucide (`@lucide/svelte`)
- **Module-level reactive state**: use `$state({ value: ... })` wrapper pattern (see `$lib/stores/`). Format helpers in `$lib/format.svelte.ts` read `appSettings.value.locale` reactively.

## Architecture

- **No `+server.ts` / `+page.server.ts`**. All server functions via SvelteKit `remoteFunctions`: `form` from `$app/server` (file upload in `$lib/db.remote.ts`) and `query` (RPC reads in `$lib/*.remote.ts` for accounts, dashboard, transactions).
- **Server-only SQLite code** in `$lib/server/`. Each function: get token → `getDbPath()` → open `better-sqlite3` readonly → query → close in `finally`.
- **Routes**: `/` (upload/file-picker), `/accounts`, `/dashboard`, `/transactions`, `/settings`, `/about`, `/license`. All SPA — no SSR data fetching.
- **compilerOptions.experimental.async: true** — top-level `await` allowed in components.
- **Desktop chrome**: `app-region: drag` title bar with min/max/close buttons — front-end only, no Tauri/Electron.

## shadcn-svelte (Nova)

- Components under `$lib/components/ui/`. Style is `nova` (see `components.json`).
- Uses `tailwind-variants` (`tv`), `bits-ui` for headless primitives.
- `$lib/utils.ts`: `cn()` (clsx + tailwind-merge).
- CSS variables in `src/routes/layout.css` (not `src/app.css`).
- Charting via `layerchart` (devDep) + custom components in `$lib/components/dashboard/`.

## SQLite / data

- Accepts **only `.gnucash` files** (GnuCash SQLite format, not generic `.db`/`.sqlite`). Max 100 MB.
- Uploaded files saved to `/tmp/spendo-uploads/`, keyed by random token, auto-pruned after 30 minutes.
- `*.db` and `*.gnucash` gitignored.

## Svelte MCP

Wired in `.opencode/opencode.json` (plugin `@sveltejs/opencode`). Use for docs lookups and code validation.

## Build artifacts

- Output goes to `/build`
- Generated `.svelte-kit/` is gitignored (regenerated via `bun run prepare` / `svelte-kit sync`)
