# spendo

Tauri v2 desktop app (Rust backend) + SvelteKit frontend. Manage your finances locally in portable `.spendo` books.

- **UI**: shadcn-svelte (Nova), Tailwind CSS v4, Lucide icons
- **Backend**: Rust (`rusqlite`) over portable `.spendo` SQLite books opened in place
- **Tooling**: Bun, TypeScript strict mode, Prettier + ESLint

## Commands

| Action      | Command                             |
| ----------- | ----------------------------------- |
| Dev server  | `bun run dev`                       |
| Build       | `bun run build`                     |
| Prepare     | `bun run prepare` (svelte-kit sync) |
| Type-check  | `bun run check`                     |
| Test        | `bun run test` (vitest)             |
| Lint        | `bun run lint`                      |
| Format      | `bun run format`                    |
| Tauri dev   | `bun run tauri:dev`                 |
| Tauri build | `bun run tauri:build`               |

## Architecture

The app works with portable `.spendo` book files (SQLite) that live wherever you choose; the last open book is remembered across restarts. Open or create a book from the welcome screen, then optionally import data from GnuCash (`.gnucash`, SQLite format) or a bank statement (`.csv`/`.txt`/`.tsv` via Settings → Import) to merge it in. Multiple imports accumulate and duplicates are skipped by GUID. Prices are fetched live from Yahoo Finance and stored in the book.

All backend calls go through `@tauri-apps/api/core` `invoke()` via `src/lib/tauri.ts`. The Rust backend lives in `src-tauri/src/` and uses `rusqlite` with Tauri's `State<Mutex<Connection>>`.

The SvelteKit frontend uses `@sveltejs/adapter-static` (SPA, no SSR).

## License

MIT. See [LICENSE.md](LICENSE.md). Copyright © 2026 Lewis Flames.
