import { Tag, Typography } from '@douyinfe/semi-ui'
import type { PoolEntry } from '../../types'
import { FORMAT_LABEL, KIND_LABEL, ROLE_LABEL } from '../../lib/mediaTags'

const { Text } = Typography

/** 池子里每行的「角色 / 形态 / 类别 / 归属状态 / 年份 / 新入库」标签行。 */
function PoolEntryMeta({ entry }: { entry: PoolEntry }) {
  return (
    <Text type="tertiary" size="small">
      {entry.role ? (
        <Tag size="small" color={entry.role === 'mix' ? 'grey' : 'yellow'} style={{ marginRight: 6 }}>
          {ROLE_LABEL[entry.role] ?? entry.role}
        </Tag>
      ) : null}
      {entry.format ? (
        <Tag size="small" color="cyan" style={{ marginRight: 6 }}>
          {FORMAT_LABEL[entry.format] ?? entry.format}
        </Tag>
      ) : null}
      {entry.kind ? (
        <Tag size="small" color="blue" style={{ marginRight: 6 }}>
          {KIND_LABEL[entry.kind] ?? entry.kind}
        </Tag>
      ) : null}
      {entry.extra ? `${entry.extra} · ` : ''}
      {entry.releaseDate ? String(entry.releaseDate).slice(0, 4) : '日期未知'}
      {entry.isNew ? (
        <Tag size="small" color="orange" style={{ marginLeft: 6 }}>
          新
        </Tag>
      ) : null}
    </Text>
  )
}

/**
 * 媒体库条目的标准行：封面 + 标题/标签 + **调用方给的行尾**。
 *
 * 池子页的条目现在渲染成封面砖块（`PoolPosterTile`），这个行组件留给"已忽略"那种
 * 需要一行一条、行尾放「恢复」的地方。
 */
export default function PoolEntryRow({
  entry,
  trailing,
}: {
  entry: PoolEntry
  trailing: React.ReactNode
}) {
  return (
    <div className="lib-suggest-row">
      <span className="lib-suggest-cover">
        {entry.coverImage ? <img src={entry.coverImage} alt="" loading="lazy" /> : null}
      </span>
      <span className="lib-suggest-meta">
        <span className="lib-suggest-title" title={entry.title}>
          {entry.title}
        </span>
        <PoolEntryMeta entry={entry} />
      </span>
      {trailing}
    </div>
  )
}
