import React from 'react'
import ReactDOM from 'react-dom/client'
import { ConfigProvider, App as AntApp } from 'antd'
import zhCN from 'antd/locale/zh_CN'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { BrowserRouter } from 'react-router'

import App from './App'
import './index.css'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: false, staleTime: 5_000 },
  },
})

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <ConfigProvider
      locale={zhCN}
      theme={{
        token: {
          // 视觉语言照 v1（`archive/v1/VISUAL_SPEC.md`）：飞书式浅色、小圆角、密一点的正文
          borderRadius: 6,
          fontSize: 13,
          controlHeight: 32,
        },
        components: {
          // 树是左栏的主体，行距调松一点（v1 的行高是 4px 上下留白 + 13px 字号）
          Tree: { titleHeight: 28, indentSize: 14 },
        },
      }}
    >
      <AntApp className="app-root">
        <QueryClientProvider client={queryClient}>
          <BrowserRouter>
            <App />
          </BrowserRouter>
        </QueryClientProvider>
      </AntApp>
    </ConfigProvider>
  </React.StrictMode>,
)
