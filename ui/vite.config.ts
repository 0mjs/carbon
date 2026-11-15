import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [tailwindcss(), react()],
  // Only for working on the page itself: Carbon (the Rust server on 3000) serves the built files, and this
  // forwards the page's API calls to it
  server: {
    port: 5173,
    strictPort: true,
    // The agents' marks are shared with the desktop app, one folder up
    fs: { allow: ['..'] },
    proxy: { '/api': { target: 'http://localhost:3000', headers: { origin: 'http://localhost:3000' } } },
  },
})
