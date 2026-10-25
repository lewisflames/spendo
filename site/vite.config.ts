import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  // The logo lives in src-tauri/icons, outside this package.
  server: { fs: { allow: ['../src-tauri/icons'] } }
});
