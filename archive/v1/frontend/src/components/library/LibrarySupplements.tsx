import { Typography } from '@douyinfe/semi-ui'
import { SOURCE_LABEL, KIND_LABEL } from '../../lib/mediaTags'
import LibraryRowMenu from './LibraryRowMenu'
import type { LibraryPanelProps } from './RelatedMediaPanel'

const { Text } = Typography

/**
 * ② 混杂内容:role=mix 的链——OST、剧情解析、预告片、攻略这类「围绕作品」的
 * 内容,条目本身不是一部作品,所以不限条数、不占正片位。空态不硬装:
 * 提示可以从待确认里挂进来。
 */
export default function LibrarySupplements({ data, busyKey, unlink, relinkRole }: LibraryPanelProps) {
  const links = data.links.mix
  return (
    <div className="lm-section">
      <div className="lm-section-head">
        <Text strong className="lm-section-title">
          混杂内容
        </Text>
        {links.length > 0 && (
          <span className="lm-count">{links.length}</span>
        )}
      </div>
      {links.length === 0 ? (
        <div className="lm-empty">
          还没有关联 OST / 解析之类的混杂内容——「待确认」里的杂乱条目可以挂进来
        </div>
      ) : (
        <div className="lm-list">
          {links.map((l) => (
            <div className="lm-row" key={l.id}>
              <span className="lm-row-cover">
                {l.coverImage ? <img src={l.coverImage} alt="" loading="lazy" /> : null}
              </span>
              <span className="lm-row-meta" title={l.title}>
                <span className="lm-row-title">{l.title}</span>
                <Text type="tertiary" size="small">
                  {SOURCE_LABEL[l.source] ?? l.source}
                  {l.kind ? ` · ${KIND_LABEL[l.kind] ?? l.kind}` : ''}
                  {l.extra ? ` · ${l.extra}` : ''}
                </Text>
              </span>
              <LibraryRowMenu
                url={l.url || undefined}
                role={l.role}
                label={l.title}
                onToggleRole={r => relinkRole(l.id, r)}
                onUnlink={() => void unlink(l.id, l.source)}
                unlinkBusy={busyKey === l.id}
              />
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
