# spendo-app

SvelteKit + Svelte 5 app. Import and explore your GnuCash financial data.

- **UI**: shadcn-svelte (Nova), Tailwind CSS v4, Lucide icons
- **Backend**: adapter-node, better-sqlite3 (persistent app database)
- **Tooling**: Bun, TypeScript strict mode, Prettier + ESLint

## Commands

| Action        | Command           |
| ------------- | ----------------- |
| Dev server    | `bun run dev`     |
| Build         | `bun run build`   |
| Preview build | `bun run preview` |
| Type-check    | `bun run check`   |
| Lint          | `bun run lint`    |
| Format        | `bun run format`  |

## Architecture

The app maintains its own SQLite database at `data/spendo.db`. Upload a `.gnucash` file (GnuCash SQLite format, max 100 MB) on the home page to import and merge your financial data. Multiple imports accumulate — duplicates are skipped by GUID.

Server functions use SvelteKit's `remoteFunctions` (`$app/server` form/query helpers) instead of `+server.ts` / `+page.server.ts`. See `src/lib/db.remote.ts`.

All routes are client-rendered — no SSR data fetching.

## Code style

- **Svelte 5 runes**: `$props()`, `$derived()`, `$state()` — no `export let`, use `onclick` not `on:click`, `{@render children()}`
- **Prettier**: tabs, single quotes, no trailing commas, print width 100
- **Tailwind v4**: `@import 'tailwindcss'` (Vite plugin, no PostCSS)
- **TypeScript**: strict mode, `moduleResolution: "bundler"`
