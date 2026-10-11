import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom'
import { AppProvider } from './lib/store'
import { LiveDraftProvider } from './lib/liveDraft'
import AppShell from './components/shell/AppShell'
import Home from './views/Home'
import WorkView from './views/WorkView'
import DocView from './views/DocView'
import RunList from './views/RunList'
import SettingsView from './views/SettingsView'
import LoginView from './views/LoginView'
import LibraryPool from './views/LibraryPool'
import StubQueue from './views/StubQueue'

export default function App() {
  return (
    <AppProvider>
      <LiveDraftProvider>
        <BrowserRouter
          future={{ v7_startTransition: true, v7_relativeSplatPath: true }}
        >
          <Routes>
            <Route element={<AppShell />}>
              <Route path="/" element={<Home />} />
              {/* UUID-first: identity is :id; optional :slug is cosmetic only */}
              <Route path="/w/:id" element={<WorkView />} />
              <Route path="/w/:id/:slug" element={<WorkView />} />
              {/* Static "folder" ranks above the dynamic :slug segment */}
              {/* 资料夹：左栏切成资料列表（SideNav 按路由判断），正文区仍打开这篇 Wiki */}
              <Route path="/w/:id/folder" element={<WorkView />} />
              <Route path="/w/:id/folder/:docId" element={<DocView />} />
              <Route path="/runs" element={<RunList />} />
              <Route path="/library" element={<LibraryPool />} />
              {/* 待写正文：节点已建、缺正文——从 /library 分出来的任务队列 */}
              <Route path="/stubs" element={<StubQueue />} />
              <Route path="/settings" element={<SettingsView />} />
              <Route path="/login" element={<LoginView />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </Route>
          </Routes>
        </BrowserRouter>
      </LiveDraftProvider>
    </AppProvider>
  )
}
