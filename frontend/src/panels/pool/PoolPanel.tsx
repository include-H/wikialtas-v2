import { useEffect, useMemo, useState } from 'react'
import { App as AntApp, Alert, Button, Empty, Input, Segmented, Spin, Tag } from 'antd'
import { ReloadOutlined, SearchOutlined } from '@ant-design/icons'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { api, type LibrarySource, type PoolEntry, type PoolQuery } from '../../api'
import BuildDocModal from './BuildDocModal'
import IgnoresModal from './IgnoresModal'
import PoolRow from './PoolRow'
import { SOURCE_LABEL, SOURCE_TAG_COLOR, groupByContainer, relative } from './util'

// 媒体库池子（`docs/features.md` §1–§2）：库状态仪表盘。
// 数据全部来自清单（秒开，不实时轰库）；写操作只有「建档 / 解链 / 忽略」三件，全是人点的。
type DocFilter = 'none' | 'has' | 'all'

export default function PoolPanel() {
  const qc = useQueryClient()
  const { message, modal } = AntApp.useApp()
  const [source, setSource] = useState<'all' | LibrarySource>('all')
  const [docFilter, setDocFilter] = useState<DocFilter>('none')
  const [keyword, setKeyword] = useState('')
  const [polling, setPolling] = useState(false)
  const [building, setBuilding] = useState<PoolEntry | null>(null)
  const [ignoresOpen, setIgnoresOpen] = useState(false)

  const status = useQuery({ queryKey: ['library-status'], queryFn: api.libraryStatus })
  const runs = useQuery({ queryKey: ['library-scan-runs'], queryFn: api.libraryScanRuns })
  const ignores = useQuery({ queryKey: ['library-ignores'], queryFn: api.libraryIgnores })

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

  const unlink = useMutation({
    mutationFn: (linkId: string) => api.unlink(linkId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['library-pool'] })
      qc.invalidateQueries({ queryKey: ['tree'] })
      message.success('已解链——文档保留，只是不再挂在库条目上')
    },
    onError: (e: Error) => message.error(e.message),
  })

  const ignore = useMutation({
    mutationFn: (p: { entry: PoolEntry; group: boolean }) =>
      api.addIgnore({
        source: p.entry.entry.source,
        external_id: p.group ? undefined : p.entry.entry.external_id,
        group_key: p.group ? p.entry.entry.container_key : undefined,
        reason: '',
      }),
    onSuccess: (_r, p) => {
      qc.invalidateQueries({ queryKey: ['library-pool'] })
      qc.invalidateQueries({ queryKey: ['library-ignores'] })
      message.success(p.group ? `已忽略整组：${p.entry.entry.container_title}` : '已忽略这一条')
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
  const latestSources = useMemo(() => {
    const seen = new Set<string>()
    return (runs.data ?? []).filter((r) => {
      if (seen.has(r.source)) return false
      seen.add(r.source)
      return true
    })
  }, [runs.data])
  const configured = status.data?.configured
  const noneConfigured = configured && !anyConfigured(configured)

  const askIgnore = (p: PoolEntry, group: boolean) => {
    modal.confirm({
      title: group ? `忽略整组「${p.entry.container_title}」？` : `忽略「${p.entry.title}」？`,
      content: '之后不再出现在池子里；可恢复（池子底部「已忽略」）。',
      okText: '忽略',
      cancelText: '算了',
      onOk: () => ignore.mutateAsync({ entry: p, group }),
    })
  }

  return (
    <div className="pool">
      <header className="pool-head">
        <div className="pool-title">
          媒体库池子
          <span className="pool-title-sub">
            {pool.data ? `${pool.data.length} 条` : ''}
            {runs.data?.[0]?.started_at ? ` · 上次扫描 ${relative(runs.data[0].started_at)}` : ' · 还没扫过'}
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
          description="密钥写 secrets 表（或设 WIKIATLAS_* 环境变量），设置页随 M9 落地。"
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
                <Tag color={SOURCE_TAG_COLOR[g.source]}>{SOURCE_LABEL[g.source]}</Tag>
                <span className="pool-group-title">{g.container_title}</span>
                {g.container_kind === 'mixed' && <Tag color="orange">混杂</Tag>}
                {g.role === 'mix' && <Tag color="orange">混杂位</Tag>}
                <span className="pool-group-count">{g.entries.length} 条</span>
                <span style={{ flex: 1 }} />
                <Button
                  type="text"
                  size="small"
                  onClick={() => askIgnore(g.entries[0], true)}
                  aria-label={`忽略整组 ${g.container_title}`}
                >
                  忽略整组
                </Button>
              </div>
              {g.entries.map((p) => (
                <PoolRow
                  key={`${p.entry.source}:${p.entry.external_id}`}
                  p={p}
                  onBuild={() => setBuilding(p)}
                  onIgnore={(group) => askIgnore(p, group)}
                  onUnlink={() => p.link_id && unlink.mutate(p.link_id)}
                />
              ))}
            </section>
          ))
        )}
      </div>

      <footer className="pool-foot">
        {latestSources.length > 0 && (
          <span className="pool-scan-summary">
            最近同步
            {latestSources.map((r) => (
              <span key={r.source} className={r.ok ? 'is-ok' : 'is-bad'}>
                {SOURCE_LABEL[r.source]} {r.ok ? '正常' : '失败'}
              </span>
            ))}
          </span>
        )}
        <span style={{ flex: 1 }} />
        <Button type="link" size="small" onClick={() => setIgnoresOpen(true)}>
          已忽略 {ignores.data?.length ?? 0}
        </Button>
      </footer>

      <BuildDocModal entry={building} open={building !== null} onClose={() => setBuilding(null)} />
      <IgnoresModal open={ignoresOpen} onClose={() => setIgnoresOpen(false)} />
    </div>
  )
}

/** 至少配了一个源。 */
function anyConfigured(c: Record<LibrarySource, boolean>): boolean {
  return c.gameatlas || c.emby || c.komga
}
