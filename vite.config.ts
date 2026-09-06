import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  clearScreen: false,
  server: {
    strictPort: true
  },
  test: {
    // Pure-helper unit tests only (no component rendering): colocated
    // `*.test.ts` next to the module, Node environment.
    include: ['src/**/*.test.ts'],
    environment: 'node'
  }
});
