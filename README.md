# spendo

Tauri v2 desktop app (Rust backend) + SvelteKit frontend. Manage your finances locally in portable `.spendo` books.

- **UI**: shadcn-svelte (Nova), Tailwind CSS v4, Lucide icons
- **Backend**: Rust (`rusqlite`) over portable `.spendo` SQLite books opened in place
- **Tooling**: Bun, TypeScript strict mode, Prettier + ESLint

## Install

### macOS

Download the latest `Spendo_<version>_universal.dmg` from the [releases page](https://github.com/lewisflames/spendo/releases) (one disk image for Apple Silicon and Intel). Builds are ad-hoc signed, not notarized: on first launch, right-click the app and choose **Open**, or allow it under System Settings → Privacy & Security.

Spendo updates itself from **About → Updates**.

### Arch Linux

`spendo-bin` is on the [AUR](https://aur.archlinux.org/packages/spendo-bin):

```sh
yay -S spendo-bin    # or paru -S spendo-bin
```

Updates come through your AUR helper (`yay -Syu spendo-bin`). The About page still reports when a newer release is out, since package-managed installs can’t self-update.

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

## Releasing

Releases are cut by pushing a `v*` tag. `.github/workflows/release.yml` then:

1. builds the universal macOS dmg (plus the `.app.tar.gz` updater artifact) and the Linux deb on GitHub runners;
2. creates a GitHub release with the bundles, signatures and the `latest.json` updater manifest;
3. repackages the deb as `spendo-bin` and pushes it to the AUR.

Version bump and release:

```sh
# bump "version" in src-tauri/tauri.conf.json (and src-tauri/Cargo.toml)
git commit -am 'chore(release): v0.2.0'
git tag v0.2.0
git push origin main --tags
```

### One-time setup

Generate the updater keypair once (the private key is what signs releases; losing it means installed apps can never update again):

```sh
bun run tauri signer generate -- -w ~/.tauri/spendo.key
```

The public key belongs in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey` and has already been wired up for this repository. Keep `~/.tauri/spendo.key` safe and out of version control.

GitHub repository secrets:

| Secret                               | Value                                              |
| ------------------------------------ | -------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | contents of `~/.tauri/spendo.key` (the whole file) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | private key password (omit if the key has none)    |
| `AUR_SSH_PRIVATE_KEY`                | SSH private key registered on your AUR account     |
| `AUR_USERNAME` / `AUR_EMAIL`         | AUR account name and commit email                  |

Local release builds need the key in the environment too:

```sh
export TAURI_SIGNING_PRIVATE_KEY="$HOME/.tauri/spendo.key"
bun run tauri:build
```

## License

MIT. See [LICENSE.md](LICENSE.md). Copyright © 2026 Lewis Flames.
