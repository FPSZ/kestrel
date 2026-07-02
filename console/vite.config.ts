import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'node:path'

// kestrel-server (axum) is expected on :4321; Vite proxies /api to it in dev.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { '@': path.resolve(__dirname, 'src') },
  },
  server: {
    port: 7823,
    proxy: {
      '/api': { target: 'http://127.0.0.1:4321', changeOrigin: true },
    },
  },
})
