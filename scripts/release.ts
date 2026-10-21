// Cut a release: bump the version, commit, tag and push. The pushed tag
// triggers .github/workflows/release.yml, which builds and publishes.
//
//   bun run release patch|minor|major|<x.y.z>

import { $ } from 'bun';

const arg = process.argv[2];
if (!arg) fail('usage: bun run release patch|minor|major|<x.y.z>');

const branch = (await $`git branch --show-current`.text()).trim();
if (branch !== 'main') fail(`releases are cut from main (on ${branch})`);
if ((await $`git status --porcelain`.text()).trim()) fail('working tree is not clean');
await $`git fetch --quiet origin main --tags`;
const [local, remote] = [await rev('HEAD'), await rev('origin/main')];
if (local !== remote) fail('main is not in sync with origin/main');

const pkg = await Bun.file('package.json').text();
const current = pkg.match(/"version": "(\d+\.\d+\.\d+)"/)?.[1];
if (!current) fail('no version in package.json');
const next = bump(current, arg);
const tag = `v${next}`;
if ((await $`git tag -l ${tag}`.text()).trim()) fail(`${tag} already exists`);

// package.json is the source of truth (tauri.conf.json points at it); Cargo
// files are kept in step so `cargo` metadata never drifts from the app.
await replaceOnce('package.json', `"version": "${current}"`, `"version": "${next}"`);
await replaceOnce('src-tauri/Cargo.toml', `version = "${current}"`, `version = "${next}"`);
await replaceOnce(
  'src-tauri/Cargo.lock',
  `name = "spendo"\nversion = "${current}"`,
  `name = "spendo"\nversion = "${next}"`
);

await $`git add package.json src-tauri/Cargo.toml src-tauri/Cargo.lock`;
await $`git commit --quiet -m ${`chore(release): ${tag}`}`;
await $`git tag -a ${tag} -m ${tag}`;
console.log(`${current} → ${next}: committed and tagged ${tag}`);

if (prompt(`Push main and ${tag} to origin now? [Y/n]`)?.trim().toLowerCase() === 'n') {
  console.log(`Not pushed. When ready: git push --atomic origin main ${tag}`);
} else {
  await $`git push --atomic origin main ${tag}`;
  console.log('Pushed. Follow the build with: gh run watch');
}

function bump(version: string, how: string): string {
  if (/^\d+\.\d+\.\d+$/.test(how)) return how;
  const [major, minor, patch] = version.split('.').map(Number);
  if (how === 'major') return `${major + 1}.0.0`;
  if (how === 'minor') return `${major}.${minor + 1}.0`;
  if (how === 'patch') return `${major}.${minor}.${patch + 1}`;
  fail(`unknown bump "${how}"`);
}

async function rev(ref: string): Promise<string> {
  return (await $`git rev-parse ${ref}`.text()).trim();
}

async function replaceOnce(path: string, from: string, to: string) {
  const text = await Bun.file(path).text();
  if (!text.includes(from)) fail(`${path}: "${from}" not found`);
  await Bun.write(path, text.replace(from, to));
}

function fail(message: string): never {
  console.error(`release: ${message}`);
  process.exit(1);
}
