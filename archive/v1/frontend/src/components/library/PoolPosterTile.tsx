import { IconCheckCircleStroked, IconImage, IconStar } from '@douyinfe/semi-icons'
import type { PoolEntry, WorkSummary } from '../../types'
import { KIND_LABEL } from '../../lib/mediaTags'
import PoolRowMenu from './PoolRowMenu'

/**
 * 容器卡里的一个封面砖块。**点一下 = 勾选**（主交互：这页的主线是"批量挑、批量建"），
 * 打开原条目移到 ⋯ 里。
 *
 * 2:3 竖版：实测三源封面 88% 是海报（GA 全是 600×900、Emby 200×300、Komga 同类），
 * 16:9 会把标题美术裁掉。少数横图（Emby 拿 backdrop 当封面的）在 onLoad 里改走 contain。
 */
export default function PoolPosterTile({
  entry,
  node,
  selected,
  onToggle,
  onLink,
  onArchive,
  onIgnore,
}: {
  entry: PoolEntry
  /** 命中"Wiki 已有这个条目"（标题或别名同名）。 */
  node?: WorkSummary
  selected: boolean
  onToggle: (e: PoolEntry) => void
  onLink: (e: PoolEntry, node: WorkSummary) => void
  onArchive: (e: PoolEntry) => void
  onIgnore: (e: PoolEntry) => void
}) {
  const year = entry.releaseDate ? String(entry.releaseDate).slice(0, 4) : ''
  const sub = [KIND_LABEL[entry.kind ?? ''] ?? entry.kind, entry.format, year].filter(Boolean).join(' · ')

  return (
    <div
      className={`pool-tile${selected ? ' is-selected' : ''}`}
      onClick={() => onToggle(entry)}
      role="checkbox"
      aria-checked={selected}
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === ' ' || e.key === 'Enter') {
          e.preventDefault()
          onToggle(entry)
        }
      }}
      title={node ? `${entry.title}（Wiki 已有《${node.title}》→ ⋯ 里可直接挂上）` : entry.title}
    >
      <div className="pool-tile-coverwrap">
        <span className="lib-poster-cover">
          {entry.coverImage ? (
            <img
              src={entry.coverImage}
              alt=""
              loading="lazy"
              onLoad={(e) => {
                // 横版封面按 2:3 裁只剩中间一条：改 contain 让它整张可见。
                // 直接改 class 不走 state——一个容器可能上百个砖块，没必要为它重渲染。
                const im = e.currentTarget
                if (im.naturalWidth > im.naturalHeight * 1.1) im.classList.add('is-wide')
              }}
            />
          ) : (
            <span className="lib-poster-ph">
              <IconImage size="large" />
            </span>
          )}
        </span>
        {/* 勾选态：角上一个实心圆点（整块也有一圈描边，见 .is-selected） */}
        <span className={`pool-tile-check${selected ? ' is-on' : ''}`} aria-hidden>
          {selected ? <IconCheckCircleStroked size="small" /> : null}
        </span>
        {node ? (
          <span className="pool-tile-badge" title={`Wiki 已有《${node.title}》`}>
            <IconStar size="extra-small" />
            Wiki 已有
          </span>
        ) : null}
        <span className="lib-row-actions pool-tile-actions">
          <PoolRowMenu
            entry={entry}
            node={node}
            onLink={onLink}
            onArchive={onArchive}
            onIgnore={onIgnore}
          />
        </span>
      </div>
      <span className="lib-poster-title" title={entry.title}>
        {entry.title}
      </span>
      <span className="lib-poster-meta">
        <span className="lib-poster-sub">{sub || '—'}</span>
        {entry.isNew ? <span className="pool-tile-new">新</span> : null}
      </span>
    </div>
  )
}
