import { useEffect, useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Breadcrumb, Button, Drawer, Grid, Tooltip, theme } from 'antd'
import {
  MenuFoldOutlined,
  MenuUnfoldOutlined,
  RightOutlined,
  ThunderboltOutlined,
} from '@ant-design/icons'

import { api } from './api'
import AiPanel from './panels/AiPanel'
import DocPanel from './panels/DocPanel'
import PoolPanel from './panels/pool/PoolPanel'
import TreePanel from './panels/TreePanel'
import { useUi } from './store'

// 骨架照 v1（`archive/v1/VISUAL_SPEC.md` §1）：左栏 280（可收到 0）、右侧 Altas 480。
// 宽度是常量、只有这一处来源；v1 的面板不可拖（用收起/展开切换），这里保持一致。
const SIDE_W = 280
const AI_W = 480
const HIDDEN = 0

export default function App() {
  const { token } = theme.useToken()
  const screens = Grid.useBreakpoint()
  const selectedId = useUi((s) => s.selectedId)
  const view = useUi((s) => s.view)
  const crumbs = useCrumbs()
  const updatedLabel = useUpdatedLabel()

  // antd 的 useBreakpoint 挂载后才订阅，首帧是 undefined：先按宽屏渲染
  const known = screens.md !== undefined
  const narrow = known && !screens.md
  const medium = known && screens.md === true && !screens.lg

  const [side, setSide] = useState(SIDE_W)
  // v1 的规矩（VISUAL_SPEC §1.5）：右侧 Altas **默认收起**，点紫色「问 Altas」才展开。
  // 默认打开会把正文挤到 1280 宽只剩 300 多像素。
  const [ai, setAi] = useState(HIDDEN)
  const [leftOpen, setLeftOpen] = useState(false)
  const [rightOpen, setRightOpen] = useState(false)

  // 断点一变就把左栏恢复到默认宽度；窄到放不下三栏时把 Altas 收起来
  // （用户还想看就再点一次「问 Altas」——这比让正文被挤到 300 多像素好）
  useEffect(() => {
    if (narrow) return
    setSide(SIDE_W)
    if (medium) setAi(HIDDEN)
  }, [narrow, medium])

  const openAi = () => (narrow ? setRightOpen(true) : setAi(AI_W))

  const actions = narrow ? (
    <>
      <Button size="small" icon={<MenuUnfoldOutlined />} onClick={() => setLeftOpen(true)}>
        知识库
      </Button>
      <Button
        size="small"
        type="text"
        className="ai-accent"
        icon={<ThunderboltOutlined />}
        onClick={openAi}
      >
        问 Altas
      </Button>
    </>
  ) : (
    <>
      <Tooltip title={side === HIDDEN ? '展开知识库' : '收起知识库'}>
        <Button
          type="text"
          size="small"
          aria-label={side === HIDDEN ? '展开知识库' : '收起知识库'}
          icon={side === HIDDEN ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />}
          onClick={() => setSide(side === HIDDEN ? SIDE_W : HIDDEN)}
        />
      </Tooltip>
      <Tooltip title={ai === HIDDEN ? '展开 Altas' : '收起 Altas'}>
        <Button
          type="text"
          size="small"
          aria-label={ai === HIDDEN ? '展开 Altas' : '收起 Altas'}
          icon={ai === HIDDEN ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />}
          onClick={() => setAi(ai === HIDDEN ? AI_W : HIDDEN)}
        />
      </Tooltip>
      <Button
        type="text"
        size="small"
        className="ai-accent"
        icon={<ThunderboltOutlined />}
        onClick={openAi}
      >
        问 Altas
      </Button>
    </>
  )

  return (
    <div
      className="shell"
      data-layout={narrow ? 'narrow' : medium ? 'medium' : 'wide'}
      style={{ ['--ai' as string]: token.purple }}
    >
      {!narrow && side !== HIDDEN && (
        <aside className="pane pane-side">
          <TreePanel />
        </aside>
      )}

      <div className="pane-main">
        <header className="top-bar">
          <div className="top-bar-crumbs">
            <Breadcrumb
              separator={<RightOutlined style={{ fontSize: 9, color: 'var(--ant-color-text-quaternary)' }} />}
              items={[{ title: '我的文档库' }, ...crumbs.map((c) => ({ title: c }))]}
            />
            {updatedLabel && <span className="top-bar-updated">最近修改：{updatedLabel}</span>}
          </div>
          <div className="top-bar-actions">{actions}</div>
        </header>

        <div className="work-area">
          <main className="pane pane-doc">
            {view === 'pool' ? <PoolPanel /> : <DocPanel nodeId={selectedId} />}
          </main>
          {!narrow && ai !== HIDDEN && (
            <aside className="pane pane-ai">
              <AiPanel />
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
              title="Altas"
              placement="right"
              width={340}
              open={rightOpen}
              onClose={() => setRightOpen(false)}
              styles={{ body: { padding: 0 } }}
            >
              <AiPanel />
            </Drawer>
          </>
        )}
      </div>
    </div>
  )
}

/** 顶栏面包屑：从扁平树往上走。用的是同一份 tree 缓存，不额外发请求。 */
function useCrumbs(): string[] {
  const selectedId = useUi((s) => s.selectedId)
  const view = useUi((s) => s.view)
  const tree = useQuery({ queryKey: ['tree'], queryFn: api.tree })
  return useMemo(() => {
    if (view === 'pool') return ['媒体库池子']
    const byId = new Map((tree.data ?? []).map((n) => [n.id, n]))
    const out: string[] = []
    let cur = selectedId ? byId.get(selectedId) : undefined
    while (cur) {
      out.unshift(cur.title)
      cur = cur.parent_id ? byId.get(cur.parent_id) : undefined
    }
    return out
  }, [tree.data, selectedId, view])
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
