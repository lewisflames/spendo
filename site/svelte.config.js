import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  compilerOptions: { runes: true },
  kit: {
    adapter: adapter({ strict: true }),
    // GitHub Pages serves the project site under /<repo>; the Pages workflow
    // passes that prefix in. Local dev and preview stay at the root.
    paths: { base: process.env.BASE_PATH ?? '' },
    // One logo source for the app and the site; screenshots come from
    // `bun run screenshots` in the app package.
    alias: {
      $icon: '../src-tauri/icons/app-icon.svg',
      $screenshots: '../docs/screenshots'
    }
  }
};

export default config;
