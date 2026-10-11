import { useState } from 'react'
import { Button, Card, Dropdown, Typography } from '@douyinfe/semi-ui'
import { IconMore } from '@douyinfe/semi-icons'
import type { PoolEntry, PoolGroup, WorkSummary } from '../../types'
import type { PoolSection } from '../../lib/poolSections'
import PoolPosterTile from './PoolPosterTile'

const { Text } = Typography

/** 一次最多渲染多少个砖块（上千张封面一起上会卡）。 */
const TILE_CAP = 60

/**
 * 一个容器（GA 系列 / Komga 系列 / Emby 库）的卡：组头 + 容器级一键 + 封面砖块。
 *
 * 容器级一键是这页的主力（逐条点 1400 条不现实）：
 *  - 容器名对上节点 → 「把这 N 条建为《X》的子节点」；
 *  - 容器是**作品系列**（GA/Komga）→ 「新建《容器名》并把这 N 条挂进去」；
 *  - Emby 的**库**容器（"电影"/"番剧"不是一个作品）→ 只能「建到…」选一个节点。
 */
export default function PoolContainerCard({
  section,
  busyKey,
  selectedKeys,
  onToggle,
  onSelectGroup,
  onLinkMany,
  onArchiveSection,
  onArchiveEntry,
  onLink,
  onIgnoreEntry,
  onIgnoreGroup,
}: {
  section: PoolSection
  busyKey: string | null
  selectedKeys: Set<string>
  onToggle: (e: PoolEntry) => void
  onSelectGroup: (items: PoolEntry[], select: boolean) => void
  /** 把这一组里 Wiki 已有节点的条目一次性挂上。 */
  onLinkMany: (items: PoolEntry[]) => void
  onArchiveSection: (s: PoolSection) => void
  onArchiveEntry: (e: PoolEntry) => void
  onLink: (e: PoolEntry, node: WorkSummary) => void
  onIgnoreEntry: (e: PoolEntry) => void
  onIgnoreGroup: (g: PoolGroup) => void
}) {
  const { group, items, matched, hitByKey, hits, seriesLike, standalone } = section
  const [limit, setLimit] = useState(TILE_CAP)
  const shown = items.slice(0, limit)
  const rest = items.length - shown.length
  const busy = busyKey === `section:${group.source}:${group.containerKey}`
  const allSelected = items.length > 0 && items.every((e) => selectedKeys.has(`${e.source}:${e.publicId}`))

  // 主按钮只在"有确定归处"时出现：
  //  · 容器名对上节点 → 建成它的子节点；
  //  · 容器是作品系列（GA/Komga）→ 先把容器名建成 series，再挂进去。
  // Emby 的**库**容器（"电影"/"番剧"不是一个作品）不给主按钮——把 88 部不相关的番剧
  // 建到同一个节点下是陷阱；那种容器走"勾选逐条"或"整组忽略"，"整组建档到…"收进 ⋯。
  const primary = matched
    ? `把这 ${items.length} 条建为《${matched.title}》的子节点`
    : seriesLike
      ? `新建《${group.containerTitle}》并把这 ${items.length} 条挂进去`
      : standalone
        ? `把这 ${items.length} 条建为待写（不归属）`
        : ''

  return (
    <Card
      className="pool-card"
      title={
        <span className="pool-container-title">
          {group.containerTitle}
          <Text type="tertiary" size="small">
            未挂链 {group.unlinked.length}
            {items.length !== group.unlinked.length ? ` · 筛出 ${items.length}` : ''}
            {hits > 0 ? ` · Wiki 已有 ${hits}` : ''}
            {group.linkedCount > 0 ? ` · 已挂 ${group.linkedCount}` : ''}
          </Text>
        </span>
      }
      headerExtraContent={
        <div className="pool-head-actions">
          {primary ? (
            <Button
              size="small"
              theme="light"
              type="primary"
              loading={busy}
              onClick={() => onArchiveSection(section)}
              title={primary}
            >
              {primary}
            </Button>
          ) : (
            <Text type="tertiary" size="small">
              勾选逐条处理，或整组忽略
            </Text>
          )}
          {hits > 0 ? (
            <Button
              size="small"
              theme="borderless"
              type="primary"
              onClick={() => onLinkMany(items)}
              title="把这一组里能在 Wiki 找到节点的条目挂上去（⋯ 里可逐条改）"
            >
              挂上已有节点（{hits}）
            </Button>
          ) : null}
          <Button size="small" theme="borderless" onClick={() => onSelectGroup(items, !allSelected)}>
            {allSelected ? '取消全选' : '全选本组'}
          </Button>
          <Dropdown
            position="bottomRight"
            trigger="click"
            render={
              <Dropdown.Menu>
                {seriesLike || matched ? null : (
                  <Dropdown.Item onClick={() => onArchiveSection(section)}>
                    整组建档到…（选节点）
                  </Dropdown.Item>
                )}
                <Dropdown.Divider />
                <Dropdown.Item type="danger" onClick={() => onIgnoreGroup(group)}>
                  整组忽略
                </Dropdown.Item>
              </Dropdown.Menu>
            }
          >
            <Button
              theme="borderless"
              type="tertiary"
              size="small"
              icon={<IconMore />}
              title={`「${group.containerTitle}」的更多操作`}
            />
          </Dropdown>
        </div>
      }
    >
      <div className="pool-tiles">
        {shown.map((e) => (
          <PoolPosterTile
            key={`${e.source}:${e.publicId}`}
            entry={e}
            node={hitByKey.get(`${e.source}:${e.publicId}`)}
            selected={selectedKeys.has(`${e.source}:${e.publicId}`)}
            onToggle={onToggle}
            onLink={onLink}
            onArchive={onArchiveEntry}
            onIgnore={onIgnoreEntry}
          />
        ))}
      </div>
      {rest > 0 ? (
        <Button
          className="pool-more"
          theme="borderless"
          type="tertiary"
          size="small"
          onClick={() => setLimit(limit + TILE_CAP * 2)}
        >
          继续显示剩余 {rest} 条
        </Button>
      ) : null}
    </Card>
  )
}
