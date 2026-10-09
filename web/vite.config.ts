import { cpSync } from 'node:fs'
import path from 'node:path'

import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig, type Plugin } from 'vite'

const root = import.meta.dirname

/** Demo build only: the voices live outside web/, the real server embeds them. */
const copyVoices = (outDir: string): Plugin => ({
  name: 'copy-voices',
  closeBundle: () => cpSync(path.resolve(root, '../assets/voices'), path.resolve(root, outDir, 'voices'), { recursive: true }),
})

// dev: `npm run dev` on :5173, API and voices proxied to gym-server on :7470
// demo: `npm run build:demo` -> dist-demo/, runs entirely in the browser (GitHub Pages)
export default defineConfig(({ mode }) => {
  const demo = mode === 'demo'
  return {
    plugins: [react(), tailwindcss(), ...(demo ? [copyVoices('dist-demo')] : [])],
    resolve: { alias: { '@': path.resolve(root, './src') } },
    base: demo ? './' : '/',
    define: { 'import.meta.env.VITE_DEMO': JSON.stringify(demo) },
    build: demo ? { outDir: 'dist-demo' } : {},
    server: {
      proxy: {
        '/api': 'http://127.0.0.1:7470',
        '/voices': 'http://127.0.0.1:7470',
      },
    },
  }
})
