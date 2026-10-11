import { useEffect, useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Button, Empty, List, Spin, Tooltip, Typography } from '@douyinfe/semi-ui'
import { IconFile, IconList, IconPlus } from '@douyinfe/semi-icons'
import type { Run, WorkSummary } from '../types'
import { getLibraryPool, listRuns } from '../lib/api'
import { useAppStore } from '../lib/store'
import { workPath } from '../lib/routes'
import { absoluteTime, ancestorPath, relativeTime } from '../lib/tree'
import { CreateNodeModal } from '../components/shell/WorkDialogs'
import type { CreateTarget } from '../components/shell/WorkDialogs'

const { Title, Text } = Typography

export default function Home() {
  const { nodes, treeLoading, treeError, me } = useAppStore()
  const nav = useNavigate()
  const [createRoot, setCreateRoot] = useState<CreateTarget | null>(null)
  const [runs, setRuns] = useState<Run[]>([])

  // 建议池只取一个数：未挂链条数，供顶部待办卡用（池快照秒开，失败就当 0，卡退到只报待建档）
  const [poolUnlinked, setPoolUnlinked] = useState(0)
  useEffect(() => {
    if (!me.authed) {
      setPoolUnlinked(0)
      return
    }
    let cancelled = false
    void getLibraryPool()
      .then((res) => {
        if (!cancelled) setPoolUnlinked(res.groups.reduce((sum, g) => sum + g.unlinked.length, 0))
      })
      .catch(() => {
        if (!cancelled) setPoolUnlinked(0)
      })
    return () => {
      cancelled = true
    }
  }, [me.authed])

  useEffect(() => {
    // 工单是私有数据：访客态不该去拉——拉了必然 401，白跑一次请求还留一条 console 错误。
    if (!me.authed) {
      setRuns([])
      return
    }
    let cancelled = false
    void (async () => {
      try {
        const res = await listRuns(undefined, 20)
        if (!cancelled) setRuns(res.runs ?? [])
      } catch {
        if (!cancelled) setRuns([])
      }
    })()
    return () => {
      cancelled = true
    }
  }, [me.authed])

  const recent = useMemo(
    () =>
      [...nodes]
        .sort((a, b) => (b.updatedAt ?? '').localeCompare(a.updatedAt ?? ''))
        .slice(0, 12),
    [nodes],
  )
  const stubs = useMemo(() => nodes.filter((n) => n.status === 'stub'), [nodes])
  const running = runs.filter((r) => r.status === 'running' || r.status === 'interrupted')

  const row = (n: WorkSummary) => {
    const path = ancestorPath(nodes, n.id)
      .slice(0, -1)
      .map((p) => p.title)
      .join(' / ')
    return (
      <List.Item
        key={n.id}
        className="doc-row"
        onClick={() => nav(workPath(n.id))}
      >
        <IconFile className="doc-row-icon" />
        <div className="doc-row-main">
          <span className="doc-row-title">{n.title}</span>
          {path && <span className="doc-row-path">{path}</span>}
        </div>
        <Tooltip content={absoluteTime(n.updatedAt)} position="left">
          <span className="doc-row-time">{relativeTime(n.updatedAt)}</span>
        </Tooltip>
      </List.Item>
    )
  }

  const section = (
    title: string,
    items: WorkSummary[],
    emptyText: string,
  ) => (
    <section className="home-section">
      <Title heading={6} className="home-section-title">
        {title}
      </Title>
      {items.length === 0 ? (
        <div className="home-empty">{emptyText}</div>
      ) : (
        <List<WorkSummary>
          dataSource={items}
          split={false}
          className="doc-list"
          renderItem={row}
        />
      )}
    </section>
  )

  return (
    <div className="home-view">
      <div className="home-inner">
      <div className="home-head">
        <Title heading={4} style={{ margin: 0 }}>
          首页
        </Title>
        {me.authed && (
          <Button
            theme="solid"
            type="primary"
            icon={<IconPlus />}
            onClick={() => setCreateRoot({ id: null, title: '', kind: null })}
          >
            新建知识库
          </Button>
        )}
      </div>

      {me.authed && (stubs.length > 0 || poolUnlinked > 0) && (
        <div className="home-pending" onClick={() => nav('/library')}>
          <IconList className="home-pending-icon" />
          <div className="home-pending-main">
            <div className="home-pending-title">媒体库建议</div>
            <div className="home-pending-desc">
              {[
                poolUnlinked > 0 ? `${poolUnlinked} 条媒体未挂链` : '',
                stubs.length > 0 ? `${stubs.length} 个待建档` : '',
              ]
                .filter(Boolean)
                .join(' · ')}
            </div>
          </div>
          <span className="home-pending-cta">去建议池&nbsp;&gt;</span>
        </div>
      )}

      {treeLoading && nodes.length === 0 && (
        <Spin style={{ display: 'block', margin: '48px auto' }} />
      )}
      {treeError && (
        <Empty description={treeError} style={{ margin: '32px 0' }} />
      )}
      {!treeLoading && !treeError && nodes.length === 0 && (
        <Empty
          className="home-blank"
          description={
            me.authed
              ? '还没有任何条目。建一个宇宙，Altas 就能按 9 章骨架开始写。'
              : '这个 Wiki 还没有公开内容'
          }
          style={{ margin: '56px 0' }}
        >
          {me.authed && (
            <Button
              theme="solid"
              type="primary"
              icon={<IconPlus />}
              onClick={() => setCreateRoot({ id: null, title: '', kind: null })}
            >
              新建知识库
            </Button>
          )}
        </Empty>
      )}

      {/* 空库时只留一个空态，不再往下铺一堆空 section */}
      {nodes.length > 0 && section('最近作品', recent, '暂无作品')}

      {me.authed && running.length > 0 && (
      <section className="home-section">
        <Title heading={6} className="home-section-title">
          进行中工单
        </Title>
        <List<Run>
          dataSource={running}
          split={false}
          className="doc-list"
          renderItem={(r) => (
            <List.Item key={r.id} className="doc-row" onClick={() => nav('/runs')}>
              <div className="doc-row-main">
                <span className="doc-row-title">{r.goal || r.intent}</span>
                <span className="doc-row-path">{r.intent}</span>
              </div>
              <span className="doc-row-time">{relativeTime(r.startedAt)}</span>
            </List.Item>
          )}
        />
      </section>
      )}

      {nodes.length > 0 && (
      <div className="home-foot">
        <Text type="tertiary" size="small">
          {me.authed ? (
            <span>
              {nodes.length} 个节点
              {stubs.length > 0 && <>{' · '}{stubs.length} 待建档</>}
            </span>
          ) : (
            `${nodes.length} 篇公开文档 · 访客模式（登录后可写作与管理）`
          )}
        </Text>
      </div>
      )}

      <CreateNodeModal target={createRoot} onClose={() => setCreateRoot(null)} />
      </div>
    </div>
  )
}
