import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  // Without this, Vite falls back to the repo root's tsconfig.json for some
  // modules, and that extends the app's generated .svelte-kit/tsconfig.json,
  // which doesn't exist in a fresh checkout.
  tsconfig: './tsconfig.json',
  // The logo lives in src-tauri/icons, outside this package.
  server: { fs: { allow: ['../src-tauri/icons'] } }
});
