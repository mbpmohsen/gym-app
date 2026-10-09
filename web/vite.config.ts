import path from 'node:path'

import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// dev: `npm run dev` on :5173, API and voices proxied to gym-server on :7470
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { '@': path.resolve(__dirname, './src') } },
  server: {
    proxy: {
      '/api': 'http://127.0.0.1:7470',
      '/voices': 'http://127.0.0.1:7470',
    },
  },
})
