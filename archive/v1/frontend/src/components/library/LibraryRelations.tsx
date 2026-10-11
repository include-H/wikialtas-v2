import { useNavigate } from 'react-router-dom'
import { Tag, Typography } from '@douyinfe/semi-ui'
import { useAppStore } from '../../lib/store'
import { workPath } from '../../lib/routes'
import type { LibraryPanelProps } from './RelatedMediaPanel'

const { Text } = Typography

const REL_LABEL: Record<string, string> = {
  adaptation_of: '改编',
  sequel_to: '续作',
  spin_off_of: '衍生',
  remake_of: '重制',
  expansion_of: '扩展',
  references: '引用',
  same_series: '同系列',
}

/**
 * ④ 作品关系:Altas 随工单自动建立的关系边(只读)。
 * 箭头表示方向:本作是 from 就是「本作 → 对方」。
 */
export default function LibraryRelations({ workId, relations }: LibraryPanelProps) {
  const nav = useNavigate()
  const { nodes } = useAppStore()
  return (
    <div className="lm-section">
      <div className="lm-section-head">
        <Text strong className="lm-section-title">
          作品关系
        </Text>
        {relations.length > 0 && <span className="lm-count">{relations.length}</span>}
      </div>
      {relations.length === 0 ? (
        <div className="lm-empty">Altas 会随工单自动建立,暂无</div>
      ) : (
        <div className="lm-list">
          {relations.map((r) => {
            // 箭头表示方向:本作是 from 就是「本作 → 对方」(与旧「关联」面板约定一致)
            const outgoing = workId === r.fromId
            const otherId = outgoing ? r.toId : r.fromId
            const title = nodes.find((n) => n.id === otherId)?.title ?? otherId.slice(0, 8)
            return (
              <div className="lm-rel-row" key={r.id}>
                <a className="lm-rel-link" onClick={() => nav(workPath(otherId))} title={title}>
                  <span className="lm-rel-arrow">{outgoing ? '→' : '←'}</span>
                  {title}
                </a>
                <Tag size="small" style={{ flex: 'none' }}>
                  {REL_LABEL[r.type] ?? r.type}
                </Tag>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
