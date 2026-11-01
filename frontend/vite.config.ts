import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  base: './',
  plugins: [tailwindcss(), svelte()],
  // The WebKit host loads one self-contained HTML document with no base URL.
  // Keep the dynamic widget import inline, never emit an external JS chunk.
  build: { rollupOptions: { output: { inlineDynamicImports: true } } },
});
