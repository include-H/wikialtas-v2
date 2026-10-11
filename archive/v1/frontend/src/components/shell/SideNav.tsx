import { useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { Button, Tooltip } from '@douyinfe/semi-ui'
import {
  IconEdit,
  IconHome,
  IconList,
  IconPlus,
  IconSearch,
  IconSidebar,
} from '@douyinfe/semi-icons'
import { useAppStore } from '../../lib/store'
import SearchModal from './SearchModal'
import WorkTree from './WorkTree'
import FolderPane from './FolderPane'
import { CreateNodeModal } from './WorkDialogs'
import type { CreateTarget } from './WorkDialogs'

export default function SideNav() {
  const nav = useNavigate()
  const loc = useLocation()
  const { nodes, sidebarCollapsed, setSidebarCollapsed, me } =
    useAppStore()
  // 「待写正文」的角标 = 已建节点但还没写正文的数量（与 /stubs 页同一个口径）。
  const stubCount = nodes.filter((n) => n.status === 'stub').length
  const [searchOpen, setSearchOpen] = useState(false)
  const [createRoot, setCreateRoot] = useState<CreateTarget | null>(null)

  // 资料夹模式：路由在 /w/:id/folder(/:docId) 时，左栏从宇宙树切成该节点的资料列表
  const folderWorkId = /^\/w\/([^/]+)\/folder/.exec(loc.pathname)?.[1] ?? null
  const folderMode =
    !!folderWorkId && folderWorkId !== '-' && nodes.some((n) => n.id === folderWorkId)

  if (sidebarCollapsed) {
    return (
      <div className="side-nav collapsed">
        <Tooltip content="WikiAltas 首页" position="right">
          <Button
            className="brand-mark"
            theme="solid"
            type="primary"
            aria-label="WikiAltas 首页"
            onClick={() => nav('/')}
          >
            W
          </Button>
        </Tooltip>
        <Tooltip content="搜索" position="right">
          <Button
            theme="borderless"
            type="tertiary"
            icon={<IconSearch />}
            onClick={() => setSearchOpen(true)}
          />
        </Tooltip>
        <Tooltip content="首页" position="right">
          <Button
            className={`rail-link${loc.pathname === '/' ? ' is-active' : ''}`}
            theme="borderless"
            type="tertiary"
            icon={<IconHome />}
            onClick={() => nav('/')}
          />
        </Tooltip>
        {me.authed && (
        <Tooltip content="新建知识库" position="right">
          <Button
            theme="borderless"
            type="tertiary"
            icon={<IconPlus />}
            onClick={() => setCreateRoot({ id: null, title: '', kind: null })}
          />
        </Tooltip>
        )}
        <div className="side-spacer" />
        <Tooltip content="展开侧边栏" position="right">
          <Button
            theme="borderless"
            type="tertiary"
            icon={<IconSidebar />}
            onClick={() => setSidebarCollapsed(false)}
          />
        </Tooltip>
        <SearchModal open={searchOpen} onClose={() => setSearchOpen(false)} />
        <CreateNodeModal target={createRoot} onClose={() => setCreateRoot(null)} />
      </div>
    )
  }

  return (
    <div className="side-nav">
      <div className="side-brand">
        <span className="brand-mark">W</span>
        <span className="brand-name">WikiAltas</span>
      </div>

      <Button
        className="side-search"
        theme="light"
        type="tertiary"
        icon={<IconSearch />}
        onClick={() => setSearchOpen(true)}
      >
        搜索
      </Button>

      <nav className="side-links">
        <Button
          className={`side-link${loc.pathname === '/' ? ' active' : ''}`}
          theme="borderless"
          type="tertiary"
          icon={<IconHome />}
          onClick={() => nav('/')}
        >
          首页
        </Button>
        <Button
          className={`side-link${loc.pathname === '/library' ? ' active' : ''}`}
          theme="borderless"
          type="tertiary"
          icon={<IconList />}
          onClick={() => nav('/library')}
        >
          媒体库建议
        </Button>
        {me.authed && (
          <Button
            className={`side-link${loc.pathname === '/stubs' ? ' active' : ''}`}
            theme="borderless"
            type="tertiary"
            icon={<IconEdit />}
            onClick={() => nav('/stubs')}
          >
            待写正文
            {stubCount > 0 ? <span className="side-count">{stubCount}</span> : null}
          </Button>
        )}
        {me.authed && (
          <Button
            className="side-link"
            theme="borderless"
            type="tertiary"
            icon={<IconPlus />}
            onClick={() => setCreateRoot({ id: null, title: '', kind: null })}
          >
            新建知识库
          </Button>
        )}
      </nav>

      <div className="side-section side-section-grow">
        <div className="side-section-head">
          {folderMode ? '资料夹' : '我的文档库'}
        </div>
        <div className="side-tree-scroll">
          {folderMode && folderWorkId ? <FolderPane workId={folderWorkId} /> : <WorkTree />}
        </div>
      </div>

      <div className="side-foot">
        <span className="side-foot-hint">
          {me.authed ? `${nodes.length} 节点` : `${nodes.length} 篇公开文档 · 访客模式`}
        </span>
        <Tooltip content="折叠侧边栏" position="top">
          <Button
            theme="borderless"
            type="tertiary"
            size="small"
            icon={<IconSidebar />}
            onClick={() => setSidebarCollapsed(true)}
          />
        </Tooltip>
      </div>

      <SearchModal open={searchOpen} onClose={() => setSearchOpen(false)} />
      <CreateNodeModal target={createRoot} onClose={() => setCreateRoot(null)} />
    </div>
  )
}
