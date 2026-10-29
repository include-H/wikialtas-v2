import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, '.', '')
  return {
    plugins: [react()],
    server: {
      port: Number(env.WIKIATLAS_WEB_PORT || 5173),
      // 端口被占时**直接报错**，别静默换端口——这台机器上曾经因此撞上一个
      // 归档前遗留的旧 vite，页面加载的 HTML 与客户端脚本不是同一份，白白查半天。
      strictPort: true,
      // 默认后端 8088；隔离验收可用 WIKIATLAS_API_TARGET 指到临时实例。
      proxy: {
        '/api': {
          target: env.WIKIATLAS_API_TARGET || 'http://127.0.0.1:8088',
          changeOrigin: true,
        },
      },
    },
  }
})
