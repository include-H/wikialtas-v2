import { useEffect, useMemo, useRef, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Breadcrumb, Button, Drawer, Dropdown, Grid, Input, Spin, Tooltip, theme } from 'antd'
import { useLocation, useNavigate } from 'react-router'
import {
  CheckOutlined,
  CloseOutlined,
  DownOutlined,
  EditOutlined,
  EyeOutlined,
  FileSearchOutlined,
  MenuUnfoldOutlined,
  RightOutlined,
  ThunderboltOutlined,
} from '@ant-design/icons'

import { api } from './api'
import AiPanel from './panels/AiPanel'
import AuditPanel from './panels/AuditPanel'
import DocPanel from './panels/DocPanel'
import HomePanel from './panels/HomePanel'
import PoolPanel from './panels/pool/PoolPanel'
import SearchPanel from './panels/SearchPanel'
import SettingsPanel from './panels/SettingsPanel'
import TreePanel from './panels/TreePanel'
import { useUi } from './store'

// 骨架照 v1（`archive/v1/VISUAL_SPEC.md` §1）：左栏 280（可收到 0）、右侧 Atlas 480。
// 宽度是常量、只有这一处来源；v1 的面板不可拖（用收起/展开切换），这里保持一致。
const SIDE_W = 280
const AI_W = 480
const HIDDEN = 0

export default function App() {
  const { token } = theme.useToken()
  const screens = Grid.useBreakpoint()
  const selectedId = useUi((s) => s.selectedId)
  const view = useUi((s) => s.view)
  const select = useUi((s) => s.select)
  const setView = useUi((s) => s.setView)
  const docMode = useUi((s) => s.docMode)
  const setDocMode = useUi((s) => s.setDocMode)
  const crumbs = useCrumbs()
  const updatedLabel = useUpdatedLabel()
  const auth = useQuery({ queryKey: ['auth'], queryFn: api.authStatus })
  const [loginPassword, setLoginPassword] = useState('')
  const [loginError, setLoginError] = useState('')
  const [loggingIn, setLoggingIn] = useState(false)

  // antd 的 useBreakpoint 挂载后才订阅，首帧是 undefined：先按宽屏渲染
  const known = screens.md !== undefined
  const narrow = known && !screens.md
  const medium = known && screens.md === true && !screens.lg

  const [side, setSide] = useState(SIDE_W)
  // v1 的规矩（VISUAL_SPEC §1.5）：右侧 Atlas **默认收起**，点紫色「问 Atlas」才展开。
  // 默认打开会把正文挤到 1280 宽只剩 300 多像素。
  const [ai, setAi] = useState(HIDDEN)
  const [leftOpen, setLeftOpen] = useState(false)
  const [rightOpen, setRightOpen] = useState(false)
  const navigate = useNavigate()
  const location = useLocation()
  const [routeReady, setRouteReady] = useState(false)
  const routePush = useRef(false)

  // URL 是主路由状态：刷新、前进/后退和直接打开 `/doc/:id` 都回到同一篇文档。
  useEffect(() => {
    const route = parseRoute(location.pathname)
    setView(route.view)
    select(route.selectedId)
    setRouteReady(true)
    // 只在首次挂载读 URL；之后由下面两个 effect 双向同步。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    if (!routeReady) return
    if (routePush.current) {
      routePush.current = false
      return
    }
    const route = parseRoute(location.pathname)
    const current = useUi.getState()
    if (current.view !== route.view) setView(route.view)
    if (current.selectedId !== route.selectedId) select(route.selectedId)
  }, [routeReady, location.pathname, select, setView])

  useEffect(() => {
    if (!routeReady) return
    const next = routePath(view, selectedId)
    if (location.pathname === next) return
    routePush.current = true
    navigate(next)
  }, [routeReady, view, selectedId, location.pathname, navigate])

  // 断点一变就把左栏恢复到默认宽度；窄到放不下三栏时把 Atlas 收起来
  // （用户还想看就再点一次「问 Atlas」——这比让正文被挤到 300 多像素好）
  useEffect(() => {
    if (narrow) return
    setSide(SIDE_W)
    if (medium) setAi(HIDDEN)
  }, [narrow, medium])

  const openAi = () => (narrow ? setRightOpen(true) : setAi(AI_W))

  if (auth.isLoading) {
    return (
      <div className="login-page">
        <Spin />
      </div>
    )
  }
  if (auth.data?.configured && !auth.data.authenticated) {
    return (
      <div className="login-page">
        <form
          className="login-card"
          onSubmit={(e) => {
            e.preventDefault()
            setLoggingIn(true)
            setLoginError('')
            void api
              .login(loginPassword)
              .then(() => window.location.reload())
              .catch((err) => setLoginError(err.message))
              .finally(() => setLoggingIn(false))
          }}
        >
          <span className="login-mark">A</span>
          <h1>Atlas</h1>
          <p>输入访问密码继续</p>
          <Input.Password
            autoFocus
            value={loginPassword}
            onChange={(e) => setLoginPassword(e.target.value)}
            placeholder="访问密码"
          />
          {loginError ? <div className="login-error">{loginError}</div> : null}
          <Button
            type="primary"
            htmlType="submit"
            block
            loading={loggingIn}
            disabled={!loginPassword}
          >
            进入
          </Button>
        </form>
      </div>
    )
  }

  const modeLabel = {
    edit: '编辑',
    proofread: '校对',
    read: '只读',
  }[docMode]
  const modeMenu = {
    items: [
      {
        key: 'edit',
        icon: docMode === 'edit' ? <CheckOutlined /> : <EditOutlined />,
        label: (
          <span className="top-mode-option">
            <strong>编辑</strong>
            <small>直接修改文档</small>
          </span>
        ),
      },
      {
        key: 'proofread',
        icon: docMode === 'proofread' ? <CheckOutlined /> : <FileSearchOutlined />,
        label: (
          <span className="top-mode-option">
            <strong>校对</strong>
            <small>默认只报告问题</small>
          </span>
        ),
      },
      {
        key: 'read',
        icon: docMode === 'read' ? <CheckOutlined /> : <EyeOutlined />,
        label: (
          <span className="top-mode-option">
            <strong>只读</strong>
            <small>禁止模型写库</small>
          </span>
        ),
      },
    ],
    onClick: ({ key }: { key: string }) =>
      setDocMode(key as 'edit' | 'proofread' | 'read'),
  }

  const actions = narrow ? (
    <>
      {selectedId && view === 'doc' && (
        <Dropdown menu={modeMenu} trigger={['click']}>
          <Button size="small" icon={<CheckOutlined />}>
            {modeLabel}
            <DownOutlined style={{ fontSize: 9 }} />
          </Button>
        </Dropdown>
      )}
      <Button size="small" icon={<MenuUnfoldOutlined />} onClick={() => setLeftOpen(true)}>
        知识库
      </Button>
      <Button
        size="small"
        type="primary"
        className="ai-top-action ai-accent"
        icon={<ThunderboltOutlined />}
        onClick={openAi}
      >
        问 Atlas
      </Button>
    </>
  ) : (
    <>
      {selectedId && view === 'doc' && (
        <Dropdown menu={modeMenu} trigger={['click']}>
          <Button type="text" size="small" className="top-mode-button">
            {modeLabel}
            <DownOutlined style={{ fontSize: 9 }} />
          </Button>
        </Dropdown>
      )}
      <Button
        type="primary"
        size="small"
        className="ai-top-action ai-accent"
        aria-label="展开 Atlas"
        icon={<ThunderboltOutlined />}
        onClick={openAi}
      >
        问 Atlas
      </Button>
      {ai !== HIDDEN && (
        <Tooltip title="收起 Atlas">
          <Button
            type="text"
            size="small"
            aria-label="收起 Atlas"
            icon={<CloseOutlined />}
            onClick={() => setAi(HIDDEN)}
          />
        </Tooltip>
      )}
    </>
  )

  return (
    <div
      className="shell"
      data-layout={narrow ? 'narrow' : medium ? 'medium' : 'wide'}
      data-ai-open={ai !== HIDDEN || rightOpen}
      style={{ ['--ai' as string]: token.purple }}
    >
      {!narrow && side !== HIDDEN && (
        <aside className="pane pane-side">
          <TreePanel onCollapse={() => setSide(HIDDEN)} />
        </aside>
      )}

      <div className="pane-main">
        <header className="top-bar">
          <div className="top-bar-left">
            {!narrow && side === HIDDEN && (
              <Tooltip title="展开知识库">
                <Button
                  type="text"
                  size="small"
                  className="side-expand"
                  aria-label="展开知识库"
                  icon={<MenuUnfoldOutlined />}
                  onClick={() => setSide(SIDE_W)}
                />
              </Tooltip>
            )}
            <div className="top-bar-crumbs">
              <Breadcrumb
                separator={<RightOutlined style={{ fontSize: 9, color: 'var(--ant-color-text-quaternary)' }} />}
                items={[
                  {
                    title: (
                      <button
                        type="button"
                        className="crumb-link"
                        onClick={() => {
                          setView('doc')
                          select(null)
                        }}
                      >
                        我的文档库
                      </button>
                    ),
                  },
                  ...crumbs.map((c) => ({
                    title: (
                      <button
                        type="button"
                        className="crumb-link"
                        onClick={() => {
                          setView('doc')
                          select(c.id)
                        }}
                      >
                        {c.title}
                      </button>
                    ),
                  })),
                ]}
              />
              {updatedLabel && <span className="top-bar-updated">最近修改：{updatedLabel}</span>}
            </div>
          </div>
          <div className="top-bar-actions">
            {actions}
            {auth.data?.configured ? (
              <Button
                type="text"
                size="small"
                onClick={() => {
                  void api.logout().then(() => window.location.reload())
                }}
              >
                退出
              </Button>
            ) : null}
          </div>
        </header>

        <div className="work-area">
          <main className="pane pane-doc">
            {view === 'pool' ? (
              <PoolPanel />
            ) : view === 'search' ? (
              <SearchPanel />
            ) : view === 'audit' ? (
              <AuditPanel />
            ) : view === 'settings' ? (
              <SettingsPanel />
            ) : selectedId ? (
              <DocPanel nodeId={selectedId} />
            ) : (
              <HomePanel />
            )}
          </main>
          {!narrow && ai !== HIDDEN && (
            <aside className="pane pane-ai">
              <AiPanel onClose={() => setAi(HIDDEN)} />
            </aside>
          )}
        </div>

        {narrow && (
          <>
            <Drawer
              title="知识库"
              placement="left"
              width={300}
              open={leftOpen}
              onClose={() => setLeftOpen(false)}
              styles={{ body: { padding: 0 } }}
            >
              <TreePanel onNavigate={() => setLeftOpen(false)} />
            </Drawer>
            <Drawer
              title={null}
              placement="right"
              width={390}
              open={rightOpen}
              onClose={() => setRightOpen(false)}
              closable={false}
              styles={{ header: { display: 'none' }, body: { padding: 0 } }}
            >
              <AiPanel onClose={() => setRightOpen(false)} />
            </Drawer>
          </>
        )}
      </div>
    </div>
  )
}

/** 顶栏面包屑：从扁平树往上走。用的是同一份 tree 缓存，不额外发请求。 */
function useCrumbs(): { id: string; title: string }[] {
  const selectedId = useUi((s) => s.selectedId)
  const view = useUi((s) => s.view)
  const tree = useQuery({ queryKey: ['tree'], queryFn: api.tree })
  return useMemo(() => {
    if (view === 'pool') return [{ id: '', title: '媒体库池子' }]
    if (view === 'search') return [{ id: '', title: '全文检索' }]
    if (view === 'audit') return [{ id: '', title: '体检与 Review' }]
    if (view === 'settings') return [{ id: '', title: '设置与用量' }]
    const byId = new Map((tree.data ?? []).map((n) => [n.id, n]))
    const out: { id: string; title: string }[] = []
    let cur = selectedId ? byId.get(selectedId) : undefined
    while (cur) {
      out.unshift({ id: cur.id, title: cur.title })
      cur = cur.parent_id ? byId.get(cur.parent_id) : undefined
    }
    return out
  }, [tree.data, selectedId, view])
}

type RouteView = 'doc' | 'pool' | 'audit' | 'search' | 'settings'

function parseRoute(pathname: string): { view: RouteView; selectedId: string | null } {
  const [head, id] = pathname.split('/').filter(Boolean)
  if (head === 'doc' && id) {
    return { view: 'doc', selectedId: decodeURIComponent(id) }
  }
  if (head === 'pool') return { view: 'pool', selectedId: null }
  if (head === 'search') return { view: 'search', selectedId: null }
  if (head === 'audit') return { view: 'audit', selectedId: null }
  if (head === 'settings') return { view: 'settings', selectedId: null }
  return { view: 'doc', selectedId: null }
}

function routePath(view: RouteView, selectedId: string | null): string {
  if (view === 'doc') return selectedId ? `/doc/${encodeURIComponent(selectedId)}` : '/'
  return `/${view}`
}

/** 「最近修改：x 分钟前」（v1 顶栏右边那行小字）。 */
function useUpdatedLabel(): string {
  const selectedId = useUi((s) => s.selectedId)
  const view = useUi((s) => s.view)
  const tree = useQuery({ queryKey: ['tree'], queryFn: api.tree })
  return useMemo(() => {
    const n = (tree.data ?? []).find((x) => x.id === selectedId)
    return view === 'doc' && n ? relativeTime(n.updated_at) : ''
  }, [tree.data, selectedId, view])
}

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime()
  const min = Math.round(diff / 60_000)
  if (min < 1) return '刚刚'
  if (min < 60) return `${min} 分钟前`
  const hour = Math.round(min / 60)
  if (hour < 24) return `${hour} 小时前`
  return `${Math.round(hour / 24)} 天前`
}
