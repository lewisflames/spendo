# Releasing

One command cuts a release from an up-to-date, clean `main`:

```sh
bun run release patch    # or minor, major, or an exact version like 1.2.0
```

It bumps the version in `package.json` (the single source; `tauri.conf.json` points at it) and in `src-tauri/Cargo.toml`/`Cargo.lock`, commits `chore(release): vX.Y.Z`, tags `vX.Y.Z` and, after a prompt, pushes both. Follow the build with `gh run watch`.

The tag triggers `.github/workflows/release.yml`, which:

1. builds the universal macOS dmg (plus the `.app.tar.gz` updater artifact) and the Linux deb;
2. publishes a GitHub release with the bundles, signatures, the `latest.json` updater manifest and notes grouped from the `feat`/`fix`/`perf` commits since the previous tag;
3. repackages the deb as `spendo-bin` and pushes it to the AUR.

A failed build can be retried from the Actions tab (**Re-run failed jobs**) without a new tag.

## One-time setup

The updater keypair signs every release. Losing the private key means installed apps can never update again, so back it up.

```sh
bun run tauri signer generate -- -w ~/.tauri/spendo.key
```

The public key is already in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.

Repository secrets:

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
