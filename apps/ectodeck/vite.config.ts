import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// In the app, the pages talk to Ectodeck's core. Opened in an ordinary
// browser during development, their /api calls go to the original editor's
// server instead, so the old and new versions can be compared side by side.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    proxy: { '/api': 'http://127.0.0.1:8910' },
  },
})
