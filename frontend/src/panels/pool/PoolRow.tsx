import { Button, Dropdown, Tag, Tooltip } from 'antd'
import { DownOutlined, PictureOutlined } from '@ant-design/icons'

import type { PoolEntry } from '../../api'
import { useUi } from '../../store'
import { KIND_LABEL } from './util'

/**
 * 池子里的一行（`docs/features.md` §2）。
 * 有文档 → 「打开正文 / 解链」；没文档 → 「建档 / 忽略（单条或整组）」。
 * 建档弹窗由父组件持有（同一时刻只开一个）。
 */
export default function PoolRow({
  p,
  onBuild,
  onIgnore,
  onUnlink,
}: {
  p: PoolEntry
  onBuild: () => void
  onIgnore: (group: boolean) => void
  onUnlink: () => void
}) {
  const select = useUi((s) => s.select)
  const setView = useUi((s) => s.setView)
  const e = p.entry
  const hasDoc = p.link_id !== null

  return (
    <div className="pool-row">
      <div className="pool-cover">
        {e.cover_url ? (
          <img src={e.cover_url} alt="" loading="lazy" />
        ) : (
          <span className="pool-cover-fallback">
            <PictureOutlined />
          </span>
        )}
      </div>
      <div className="pool-row-main">
        <div
          className={`pool-row-title${hasDoc ? ' is-link' : ''}`}
          title={e.title}
          onClick={() => {
            if (!p.node_id) return
            setView('doc')
            select(p.node_id)
          }}
        >
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
          <>
            <Tooltip title={p.node_title ? `已挂到：${p.node_title}` : '已挂链'}>
              <Tag color="green">有文档</Tag>
            </Tooltip>
            <Button size="small" onClick={onUnlink}>
              解链
            </Button>
          </>
        ) : (
          <>
            <Tag>无文档</Tag>
            <Button size="small" type="primary" onClick={onBuild}>
              建档
            </Button>
            <Dropdown
              menu={{
                items: [
                  { key: 'one', label: '忽略这一条' },
                  { key: 'group', label: `忽略整组「${e.container_title}」` },
                ],
                onClick: ({ key }) => onIgnore(key === 'group'),
              }}
            >
              <Button size="small" icon={<DownOutlined />} aria-label="更多" />
            </Dropdown>
          </>
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
