import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    // 端口被占时**直接报错**，别静默换端口——这台机器上曾经因此撞上一个
    // 归档前遗留的旧 vite，页面加载的 HTML 与客户端脚本不是同一份，白白查半天。
    strictPort: true,
    // 后端在 8088；前端一律走 /api，免得跨域与硬编码地址
    proxy: { '/api': { target: 'http://127.0.0.1:8088', changeOrigin: true } },
  },
})
