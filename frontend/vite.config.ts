import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  base: './',
  plugins: [tailwindcss(), svelte()],
  // The WebKit host loads one self-contained HTML document with no base URL.
  // Keep the dynamic widget import inline, never emit an external JS chunk.
  build: {
    // Vite's module-preload helper emits import.meta.url, invalid inside
    // the classic <script> used by the self-contained WebKit host.
    modulePreload: false,
    rollupOptions: { output: { inlineDynamicImports: true } },
  },
});
