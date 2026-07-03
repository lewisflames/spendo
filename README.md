# spendo-app

SvelteKit + Svelte 5 app. GnuCash financial data explorer.

- **UI**: shadcn-svelte (Nova), Tailwind CSS v4, Lucide icons, LayerChart
- **Backend**: adapter-node, better-sqlite3 (read-only queries on uploaded GnuCash databases)
- **Tooling**: Bun, TypeScript strict mode, Prettier + ESLint

## Getting started

```sh
bun install
bun run dev
```

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

Server functions use SvelteKit's `remoteFunctions` (`$app/server` form/query helpers) instead of `+server.ts` / `+page.server.ts`. See `src/lib/db.remote.ts`.

All routes are client-rendered — no SSR data fetching. Upload a `.gnucash` file (GnuCash SQLite format, max 100 MB) on the home page. Files are saved to `/tmp/spendo-uploads/` and auto-purged after 30 minutes. No persistent database — start fresh each session.

## Code style

- **Svelte 5 runes**: `$props()`, `$derived()`, `$state()` — no `export let`, use `onclick` not `on:click`, `{@render children()}`
- **Prettier**: tabs, single quotes, no trailing commas, print width 100
- **Tailwind v4**: `@import 'tailwindcss'` (Vite plugin, no PostCSS)
- **TypeScript**: strict mode, `moduleResolution: "bundler"`
