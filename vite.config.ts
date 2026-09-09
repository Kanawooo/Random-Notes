import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    host: true, // 同时监听 IPv4/IPv6；默认 localhost 在 Node 17+ 只绑 ::1，Tauri 按 127.0.0.1 探活会卡 Waiting
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**']
    }
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src')
    }
  },
  build: {
    target: 'es2022',
    minify: 'esbuild',
    sourcemap: false
  }
})
