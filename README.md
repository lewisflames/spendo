# spendo

Tauri v2 desktop app (Rust backend) + SvelteKit frontend. Import and explore your GnuCash financial data.

- **UI**: shadcn-svelte (Nova), Tailwind CSS v4, Lucide icons
- **Backend**: Rust (`rusqlite`), persistent SQLite database in Tauri app data directory
- **Tooling**: Bun, TypeScript strict mode, Prettier + ESLint

## Commands

| Action        | Command                             |
| ------------- | ----------------------------------- |
| Dev server    | `bun run dev`                       |
| Build         | `bun run build`                     |
| Preview build | `bun run preview`                   |
| Prepare       | `bun run prepare` (svelte-kit sync) |
| Type-check    | `bun run check`                     |
| Lint          | `bun run lint`                      |
| Format        | `bun run format`                    |
| Tauri dev     | `bun run tauri:dev`                 |
| Tauri build   | `bun run tauri:build`               |

## Architecture

The app maintains its own SQLite database in Tauri's app data directory (`spendo.db`). Upload a `.gnucash` file (GnuCash SQLite format) via the file dialog on the home page to import and merge your financial data. Multiple imports accumulate and duplicates are skipped by GUID.

All backend calls go through `@tauri-apps/api/core` `invoke()` via `src/lib/tauri.ts`. The Rust backend lives in `src-tauri/src/` and uses `rusqlite` with Tauri's `State<Mutex<Connection>>`.

The SvelteKit frontend uses `@sveltejs/adapter-static` (SPA, no SSR).

## License

MIT. See [LICENSE.md](LICENSE.md). Copyright © 2026 Lewis Flames.
