import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // src-tauri/target 안의 실행 파일은 빌드 중 잠기므로 감시 대상에서 뺀다.
    watch: { ignored: ['**/src-tauri/**'] },
  },
  build: { target: 'chrome110', outDir: 'dist' },
})
