import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Banner, Button, Empty, Input, Select, Spin, Typography } from '@douyinfe/semi-ui'
import { IconAIFilledLevel1, IconSearch } from '@douyinfe/semi-icons'
import type { PoolEntry, PoolGroup, WorkSummary } from '../types'
import { useAppStore } from '../lib/store'
import { useLibraryPool } from '../lib/useLibraryPool'
import { buildTreeData, relativeTime } from '../lib/tree'
import { indexWikiNodes, matchNode, splitPoolSections, type PoolSection } from '../lib/poolSections'
import PoolBulkBar from '../components/library/PoolBulkBar'
import PoolContainerCard from '../components/library/PoolContainerCard'
import PoolIgnoredCard from '../components/library/PoolIgnoredCard'
import PoolOtherSources from '../components/library/PoolOtherSources'
import { PoolPickModal } from '../components/library/PoolModals'

const { Text, Title } = Typography

/** 「建为待写」弹窗的待办：单条 / 一组 / 多选一批，都走同一个选节点弹窗。 */
type Pick = { subject: string; count: number; entries: PoolEntry[] }

/**
 * 媒体库建议——**进料台**。
 *
 * 一件事：把库里的东西快速转成待写清单。
 *   库里挑（勾选） → 建为待写（stub + 链） → 「待写正文」交给 Altas 写 → 产出 wiki
 *
 * 所以这页的主交互是**勾选 + 批量**，不是逐条点「建档」。这里没有"AI 建议"这个对象：
 * 树只有几十个节点、库里上千条未挂链，"这条该挂到哪"在大多数条目上问不出来；
 * 而"这条值不值得写"是你的取舍。模型的位置在下游——写正文。
 * 「Wiki 已有」的判定是**确定性**的：节点标题或别名与条目标题精确同名（见 poolSections）。
 */
export default function LibraryPool() {
  const nav = useNavigate()
  const { me, nodes } = useAppStore()
  const p = useLibraryPool()
  const data = p.data

  const [query, setQuery] = useState('')
  const [wikiFilter, setWikiFilter] = useState<'all' | 'hit' | 'new'>('all')
  const [sourceFilter, setSourceFilter] = useState<string>('all')
  const [sel, setSel] = useState<Set<string>>(new Set())
  const [pick, setPick] = useState<Pick | null>(null)
  const [pickParent, setPickParent] = useState<string | undefined>(undefined)

  const keyOf = (e: PoolEntry) => `${e.source}:${e.publicId}`
  const index = useMemo(() => indexWikiNodes(nodes), [nodes])
  const treeData = useMemo(() => buildTreeData(nodes), [nodes])
  const stubCount = nodes.filter((n) => n.status === 'stub').length

  // 页级过滤（搜索 / 来源 / Wiki 有没有）——三段共用
  const visible = useMemo(() => {
    const q = query.trim().toLowerCase()
    return (g: PoolGroup) =>
      g.unlinked.filter((e) => {
        if (sourceFilter !== 'all' && e.source !== sourceFilter) return false
        if (q && !e.title.toLowerCase().includes(q)) return false
        const hit = !!matchNode(e.title, index) || !!matchNode(g.containerTitle, index)
        if (wikiFilter === 'hit' && !hit) return false
        if (wikiFilter === 'new' && hit) return false
        return true
      })
  }, [query, sourceFilter, wikiFilter, index])

  const { featured, restBySource } = useMemo(
    () => splitPoolSections(data?.groups ?? [], nodes, visible),
    [data, nodes, visible],
  )

  // 选中集合按 key 存（筛选变化不丢选中），动作时再换回条目对象
  const entryByKey = useMemo(() => {
    const m = new Map<string, PoolEntry>()
    const c = new Map<string, string>()
    for (const g of data?.groups ?? []) {
      for (const e of g.unlinked) {
        m.set(keyOf(e), e)
        c.set(keyOf(e), `${g.source}|${g.containerKey}`)
      }
    }
    return { items: m, containers: c }
  }, [data])
  const selected = useMemo(
    () => [...sel].map((k) => entryByKey.items.get(k)).filter((e): e is PoolEntry => !!e),
    [sel, entryByKey],
  )
  const selectedContainers = new Set(selected.map((e) => entryByKey.containers.get(keyOf(e)))).size
  const selectedHits = selected.filter((e) => matchNode(e.title, index)).length

  if (!me.authed) {
    return <Empty description="媒体库建议需要登录后查看" style={{ margin: 64 }} />
  }

  const toggle = (e: PoolEntry) =>
    setSel((prev) => {
      const n = new Set(prev)
      const k = keyOf(e)
      if (n.has(k)) n.delete(k)
      else n.add(k)
      return n
    })
  const selectGroup = (items: PoolEntry[], on: boolean) =>
    setSel((prev) => {
      const n = new Set(prev)
      for (const e of items) {
        const k = keyOf(e)
        if (on) n.add(k)
        else n.delete(k)
      }
      return n
    })
  const clearSel = () => setSel(new Set())

  const openPick = (entries: PoolEntry[], subject: string) => {
    setPick({ subject, count: entries.length, entries })
    setPickParent(undefined)
  }

  /** 容器级一键：能确定归处的直接建（后端按规则落位）；库容器（"番剧"）反过来问一次。 */
  const archiveSection = (s: PoolSection) => {
    // 占位容器（GA 里没有系列的散游戏）：没有系列可挂 → 按"不归属"直接建节点。
    if (s.standalone) {
      void p.archiveEntries(s.items, { parentId: '' })
      return
    }
    if (!p.archiveSection(s)) openPick(s.items, s.group.containerTitle)
  }

  const archiveEntry = (e: PoolEntry) => openPick([e], e.title)
  const linkEntry = (e: PoolEntry, node: WorkSummary) => void p.doLink(e, node.id)

  const onPickOk = () => {
    if (!pick || !pickParent) return
    const cur = pick
    setPick(null)
    clearSel()
    // 弹窗没选节点 = 「不归属」（顶级）
    void p.archiveEntries(cur.entries, { parentId: pickParent })
  }

  const unlinked = (data?.groups ?? []).reduce((n, g) => n + g.unlinked.length, 0)
  const linked = (data?.groups ?? []).reduce((n, g) => n + g.linkedCount, 0)

  const sectionProps = {
    busyKey: p.busy,
    selectedKeys: sel,
    onToggle: toggle,
    onSelectGroup: selectGroup,
    onLinkMany: (items: PoolEntry[]) => void p.linkMany(items).then(clearSel),
    onArchiveSection: archiveSection,
    onArchiveEntry: archiveEntry,
    onLink: linkEntry,
    onIgnoreEntry: (e: PoolEntry) => void p.ignoreEntry(e),
    onIgnoreGroup: p.ignoreGroup,
  }

  return (
    <div className="pool-page">
      <div className="pool-inner">
        <div className="pool-head">
          <Title heading={4} style={{ margin: 0 }}>
            媒体库建议
          </Title>
          <div className="pool-head-btns">
            <Button loading={p.scanning} onClick={() => void p.runScan()}>
              扫描媒体库
            </Button>
            <Button
              theme="solid"
              type="primary"
              icon={<IconAIFilledLevel1 />}
              className="ai-coedit-btn"
              disabled={stubCount === 0}
              onClick={() => nav('/stubs')}
              title="已建节点、还没写正文——交给 Altas 按骨架写"
            >
              待写正文 {stubCount} 篇 → 交给 Altas 写
            </Button>
          </div>
        </div>
        <Text type="tertiary" size="small">
          {data?.scannedAt
            ? `库里 ${unlinked} 条未挂链 · 已挂 ${linked} · 已忽略 ${data.ignoredEntries.length} · 扫描于 ${relativeTime(data.scannedAt)}`
            : '还没扫描过——点「扫描媒体库」把 Emby / Komga / GameAtlas 的清单拉成快照。'}
        </Text>

        {p.loading && !data ? (
          <Spin style={{ display: 'block', margin: '48px auto' }} />
        ) : !data ? (
          <Empty style={{ margin: 48 }} description="读取失败" />
        ) : !data.scannedAt ? (
          <Banner
            type="info"
            closeIcon={null}
            style={{ marginTop: 12 }}
            description="扫描后：这里按「来源 → 系列/库」列出还没挂链的条目。勾选想写的 →「建为待写」（会在 Wiki 里建节点并挂上库条目）→ 到「待写正文」交给 Altas 写。"
          />
        ) : (
          <>
            {data.sourcesConfigured?.emby && !data.rolesConfigured && (
              <Banner
                type="info"
                closeIcon={null}
                style={{ marginTop: 12 }}
                description="还没给 Emby 媒体库标角色——去设置页把「正片 / 混杂内容」点出来，扫描才会收录它们。"
              />
            )}

            <div className="pool-toolbar">
              <Input
                size="small"
                prefix={<IconSearch />}
                placeholder="搜索条目…"
                showClear
                value={query}
                onChange={setQuery}
                style={{ width: 240 }}
              />
              <Select<string>
                size="small"
                value={wikiFilter}
                onChange={(v) => setWikiFilter(String(v) as 'all' | 'hit' | 'new')}
                optionList={[
                  { value: 'all', label: '全部' },
                  { value: 'hit', label: 'Wiki 已有节点' },
                  { value: 'new', label: 'Wiki 还没有' },
                ]}
                style={{ width: 150 }}
              />
              <Select<string>
                size="small"
                value={sourceFilter}
                onChange={(v) => setSourceFilter(String(v))}
                optionList={[
                  { value: 'all', label: '全部来源' },
                  { value: 'gameatlas', label: 'GameAtlas' },
                  { value: 'emby', label: 'Emby' },
                  { value: 'komga', label: 'Komga' },
                ]}
                style={{ width: 130 }}
              />
            </div>

            {featured.length === 0 && restBySource.length === 0 ? (
              <Empty style={{ margin: 48 }} description="没有未挂链的库条目" />
            ) : null}

            {featured.length > 0 && (
              <div className="pool-section">
                <Text type="tertiary" size="small" className="pool-section-head">
                  有现成归处 · {featured.length} 个容器（容器名对上了节点，或组里有条目在 Wiki 已有）
                </Text>
                {featured.map((s) => (
                  <PoolContainerCard
                    key={`${s.group.source}|${s.group.containerKey}`}
                    section={s}
                    {...sectionProps}
                  />
                ))}
              </div>
            )}

            <PoolOtherSources restBySource={restBySource}>
              {(s) => <PoolContainerCard key={`${s.group.source}|${s.group.containerKey}`} section={s} {...sectionProps} />}
            </PoolOtherSources>

            <PoolIgnoredCard
              entries={data.ignoredEntries}
              groups={data.ignoredGroups}
              onRestoreEntry={(e) => void p.restoreEntry(e)}
              onRestoreGroup={(source, key) => void p.restoreGroup(source, key)}
            />
          </>
        )}

        <PoolBulkBar
          count={selected.length}
          containers={selectedContainers}
          hitCount={selectedHits}
          busy={p.busy}
          onArchive={() => openPick(selected, `${selected.length} 条`)}
          onLinkExisting={() => void p.linkMany(selected).then(clearSel)}
          onIgnore={() => void p.ignoreEntries(selected).then(clearSel)}
          onClear={clearSel}
        />

        <PoolPickModal
          open={!!pick}
          mode="archive"
          subject={pick?.subject ?? ''}
          count={pick?.count ?? 1}
          treeData={treeData}
          pickParent={pickParent}
          onPickParentChange={setPickParent}
          onCancel={() => setPick(null)}
          onOk={onPickOk}
        />
      </div>
    </div>
  )
}
