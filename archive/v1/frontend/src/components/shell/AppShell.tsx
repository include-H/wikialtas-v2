import { Outlet } from 'react-router-dom'
import { useEffect } from 'react'
import { useLocation } from 'react-router-dom'
import { Layout } from '@douyinfe/semi-ui'
import SideNav from './SideNav'
import TopBar from './TopBar'
import RightSidebar from './RightSidebar'
import { useAppStore } from '../../lib/store'

export default function AppShell() {
  const { setAiPanelOpen, sidebarCollapsed, setActiveBatchId } = useAppStore()
  const { me } = useAppStore()
  const loc = useLocation()

  // 深链：?ai=1 展开右侧栏（Altas 标签）；?batch=<id> 打开批次卡（两者都能用于外部链接）
  useEffect(() => {
    const params = new URLSearchParams(loc.search)
    if (params.get('ai') === '1') setAiPanelOpen(true)
    const batch = params.get('batch')
    if (batch) setActiveBatchId(batch)
  }, [loc.search, setAiPanelOpen, setActiveBatchId])

  return (
    <Layout hasSider className="app-shell">
      <Layout.Sider
        className={`shell-side${sidebarCollapsed ? ' is-collapsed' : ''}`}
        style={{ width: sidebarCollapsed ? 48 : 280 }}
      >
        <SideNav />
      </Layout.Sider>
      <Layout.Content className="shell-main">
        <TopBar />
        <div className="shell-work">
          <div className="shell-body">
            <Outlet />
          </div>
          {me.authed && <RightSidebar />}
        </div>
      </Layout.Content>
    </Layout>
  )
}
