import { useEffect, useMemo, useState } from 'react'
import { App as AntApp, Alert, Button, Empty, Input, Segmented, Spin, Tag, Tooltip } from 'antd'
import { ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { api, type LibrarySource, type PoolEntry, type PoolQuery } from '../api'

// 媒体库池子（`docs/features.md` §1–§2）：库状态仪表盘。
// 全部数据来自清单（秒开，不实时轰库）；M1 只读——建档/挂链在 M2。
const SOURCE_LABEL: Record<LibrarySource, string> = {
  gameatlas: 'GameAtlas',
  emby: 'Emby',
  komga: 'Komga',
}
const KIND_LABEL: Record<string, string> = {
  game: '游戏',
  movie: '电影',
  tv: '剧集',
  book: '书',
  collection: '合集',
}
type DocFilter = 'none' | 'has' | 'all'

export default function PoolPanel() {
  const qc = useQueryClient()
  const { message } = AntApp.useApp()
  const [source, setSource] = useState<'all' | LibrarySource>('all')
  const [docFilter, setDocFilter] = useState<DocFilter>('none')
  const [keyword, setKeyword] = useState('')
  const [polling, setPolling] = useState(false)

  const status = useQuery({ queryKey: ['library-status'], queryFn: api.libraryStatus })
  const runs = useQuery({ queryKey: ['library-scan-runs'], queryFn: api.libraryScanRuns })

  const query: PoolQuery = {
    source: source === 'all' ? undefined : source,
    has_doc: docFilter === 'none' ? false : docFilter === 'has' ? true : undefined,
  }
  const pool = useQuery({
    queryKey: ['library-pool', source, docFilter],
    queryFn: () => api.libraryPool(query),
  })

  const scan = useMutation({
    mutationFn: () => api.libraryScan(source === 'all' ? undefined : [source]),
    onSuccess: () => {
      message.info('已入队：扫描在 worker 里跑，跑完这里自动刷新')
      setPolling(true)
    },
    onError: (e: Error) => message.error(e.message),
  })

  // 扫描期间轮询队列深度；空了就一次性刷新池子与记账。
  useEffect(() => {
    if (!polling) return
    const t = window.setInterval(async () => {
      const s = await qc.fetchQuery({ queryKey: ['library-status'], queryFn: api.libraryStatus })
      if (s.queueDepth === 0) {
        window.clearInterval(t)
        setPolling(false)
        qc.invalidateQueries({ queryKey: ['library-pool'] })
        qc.invalidateQueries({ queryKey: ['library-scan-runs'] })
      }
    }, 2500)
    return () => window.clearInterval(t)
  }, [polling, qc])

  const groups = useMemo(() => groupByContainer(pool.data ?? [], keyword), [pool.data, keyword])
  const configured = status.data?.configured
  const noneConfigured = configured && !configured.gameatlas && !configured.emby && !configured.komga

  return (
    <div className="pool">
      <header className="pool-head">
        <div className="pool-title">
          媒体库池子
          <span className="pool-title-sub">
            {pool.data ? `${pool.data.length} 条` : ''}
            {runs.data?.[0]?.started_at
              ? ` · 上次扫描 ${relative(runs.data[0].started_at)}`
              : ' · 还没扫过'}
          </span>
        </div>
        <div className="pool-tools">
          <Input
            size="small"
            allowClear
            prefix={<SearchOutlined />}
            placeholder="过滤标题"
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            style={{ width: 160 }}
          />
          <Segmented
            size="small"
            value={source}
            onChange={(v) => setSource(v as 'all' | LibrarySource)}
            options={[
              { label: '全部源', value: 'all' },
              { label: 'GA', value: 'gameatlas' },
              { label: 'Emby', value: 'emby' },
              { label: 'Komga', value: 'komga' },
            ]}
          />
          <Segmented
            size="small"
            value={docFilter}
            onChange={(v) => setDocFilter(v as DocFilter)}
            options={[
              { label: '无文档', value: 'none' },
              { label: '有文档', value: 'has' },
              { label: '全部', value: 'all' },
            ]}
          />
          <Button
            size="small"
            type="primary"
            icon={<ReloadOutlined />}
            loading={scan.isPending || polling}
            onClick={() => scan.mutate()}
          >
            扫描媒体库
          </Button>
        </div>
      </header>

      {noneConfigured && (
        <Alert
          type="warning"
          showIcon
          style={{ margin: '0 16px 8px' }}
          message="三个源都还没配"
          description="M1 的临时入口：密钥写 secrets 表（或设 WIKIATLAS_* 环境变量），设置页随 M9 落地。"
        />
      )}

      <div className="pool-body">
        {pool.isLoading ? (
          <div className="pool-center">
            <Spin />
          </div>
        ) : groups.length === 0 ? (
          <div className="pool-center">
            {runs.data?.length ? (
              <Empty description="这个筛选下没有条目" />
            ) : (
              <Empty description="还没扫过——点右上角「扫描媒体库」" />
            )}
          </div>
        ) : (
          groups.map((g) => (
            <section key={g.key} className="pool-group">
              <div className="pool-group-head">
                <Tag color={g.source === 'gameatlas' ? 'blue' : g.source === 'emby' ? 'geekblue' : 'green'}>
                  {SOURCE_LABEL[g.source]}
                </Tag>
                <span className="pool-group-title">{g.container_title}</span>
                {g.container_kind === 'mixed' && <Tag color="orange">混杂</Tag>}
                {g.role === 'mix' && <Tag color="orange">混杂位</Tag>}
                <span className="pool-group-count">{g.entries.length} 条</span>
              </div>
              {g.entries.map((p) => (
                <PoolRow key={`${p.entry.source}:${p.entry.external_id}`} p={p} />
              ))}
            </section>
          ))
        )}
      </div>

      <footer className="pool-foot">
        {runs.data && runs.data.length > 0 && (
          <span>
            最近一次：{runs.data.map((r) => `${r.source} ${r.ok ? '✓' : '✗'}`).join(' · ')}
          </span>
        )}
        <span style={{ flex: 1 }} />
        <span>建档 / 挂链在 M2</span>
      </footer>
    </div>
  )
}

function PoolRow({ p }: { p: PoolEntry }) {
  const e = p.entry
  const hasDoc = p.link_id !== null
  return (
    <div className="pool-row">
      <div className="pool-cover">
        {e.cover_url ? (
          <img src={e.cover_url} alt="" loading="lazy" />
        ) : (
          <span>{e.title.slice(0, 1)}</span>
        )}
      </div>
      <div className="pool-row-main">
        <div className="pool-row-title" title={e.title}>
          {e.title}
          {e.year && <span className="pool-year">{e.year}</span>}
        </div>
        <div className="pool-row-sub">
          <Tag>{KIND_LABEL[e.kind] ?? e.kind}</Tag>
          {e.format && <Tag>{e.format === 'novel' ? '小说' : '漫画'}</Tag>}
          {e.extra && <span className="pool-extra">{e.extra}</span>}
        </div>
      </div>
      <div className="pool-row-right">
        {hasDoc ? (
          <Tooltip title={p.node_title ? `已挂到：${p.node_title}` : '已挂链'}>
            <Tag color="green">有文档</Tag>
          </Tooltip>
        ) : (
          <Tag>无文档</Tag>
        )}
        {e.url && (
          <a href={e.url} target="_blank" rel="noreferrer" className="pool-open">
            打开
          </a>
        )}
      </div>
    </div>
  )
}

interface Group {
  key: string
  source: LibrarySource
  container_title: string
  container_kind: string
  role: string
  entries: PoolEntry[]
}

/** 按「源 → 容器」分组；keyword 只过滤标题（本地过滤，清单已在手）。 */
function groupByContainer(rows: PoolEntry[], keyword: string): Group[] {
  const kw = keyword.trim().toLowerCase()
  const kept = kw
    ? rows.filter(
        (p) =>
          p.entry.title.toLowerCase().includes(kw) ||
          p.entry.aliases.some((a) => a.toLowerCase().includes(kw)),
      )
    : rows
  const map = new Map<string, Group>()
  for (const p of kept) {
    const key = `${p.entry.source}|${p.entry.container_key}`
    let g = map.get(key)
    if (!g) {
      g = {
        key,
        source: p.entry.source,
        container_title: p.entry.container_title,
        container_kind: p.entry.container_kind,
        role: p.role,
        entries: [],
      }
      map.set(key, g)
    }
    g.entries.push(p)
  }
  return [...map.values()].sort((a, b) => b.entries.length - a.entries.length)
}

function relative(iso: string): string {
  const min = Math.round((Date.now() - new Date(iso).getTime()) / 60_000)
  if (min < 1) return '刚刚'
  if (min < 60) return `${min} 分钟前`
  const h = Math.round(min / 60)
  return h < 24 ? `${h} 小时前` : `${Math.round(h / 24)} 天前`
}
