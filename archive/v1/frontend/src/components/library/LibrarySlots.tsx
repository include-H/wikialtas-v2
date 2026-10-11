import { useState } from 'react'
import { Button, Typography } from '@douyinfe/semi-ui'
import { IconSourceControl, IconVideo, IconBook } from '@douyinfe/semi-icons'
import type { LibraryApiSource } from '../../lib/api'
import { SOURCE_LABEL, LINKED_BY_LABEL, homeSourceOf } from '../../lib/mediaTags'
import type { LibraryPanelProps } from './RelatedMediaPanel'
import LibraryLinkModal from './LibraryLinkModal'
import LibraryRowMenu from './LibraryRowMenu'

const { Text } = Typography

const SLOT_ORDER: { source: LibraryApiSource; label: string; icon: React.ReactNode }[] = [
  { source: 'gameatlas', label: '游戏', icon: <IconSourceControl /> },
  { source: 'emby', label: '影像', icon: <IconVideo /> },
  { source: 'komga', label: '书', icon: <IconBook /> },
]

/**
 * ① 正片位:作品只有一个介质,正片条目只可能落在对应的库——
 * 电影/剧集 → Emby,游戏 → GameAtlas,书/漫画 → Komga。所以这里只渲染
 * 介质对应的那一个槽(介质不明时才把三个槽都摆出来)。装 role=work 的链,
 * 一个槽可以有多条(同一作品的 v1/v2),挂错时用行末「⋯→设为混杂」改判。
 * 空槽 / 已有链时都留「关联…」入口(全库搜索弹窗;GA 一条即满,不给追加)。
 * 「本家」不必标了——剩下的就是本家。
 */
export default function LibrarySlots({ data, busyKey, unlink, relinkRole, reload }: LibraryPanelProps) {
  const [linkSource, setLinkSource] = useState<LibraryApiSource | null>(null)
  const homeSource = homeSourceOf(data.work.medium)
  const slots = homeSource ? SLOT_ORDER.filter(s => s.source === homeSource) : SLOT_ORDER

  return (
    <div className="lm-section">
      <div className="lm-section-head">
        <Text strong className="lm-section-title">
          正片位
        </Text>
        <Text type="tertiary" size="small">
          {homeSource ? '这部作品的本体条目' : '在各库里的本体条目'}
        </Text>
      </div>
      {slots.map((slot) => {
        const isHome = slot.source === homeSource
        const links = data.links.work.filter((l) => l.source === slot.source)
        return (
          <div className={`lm-slot${isHome ? ' is-home' : ''}`} key={slot.source}>
            {slots.length > 1 && (
              <div className="lm-slot-label">
                <span className="lm-slot-icon">{slot.icon}</span>
                {slot.label}
              </div>
            )}
            {links.length === 0 ? (
              <Button size="small" theme="borderless" onClick={() => setLinkSource(slot.source)}>
                关联到 {SOURCE_LABEL[slot.source]}…
              </Button>
            ) : (
              <>
                {links.map((l) => (
                  <div className="lm-row" key={l.id}>
                    <span className="lm-row-cover">
                      {l.coverImage ? <img src={l.coverImage} alt="" loading="lazy" /> : null}
                    </span>
                    <span className="lm-row-meta" title={l.title}>
                      <span className="lm-row-title">{l.title}</span>
                      <Text type="tertiary" size="small">
                        {LINKED_BY_LABEL[l.linkedBy ?? ''] ?? l.linkedBy}
                        {l.extra ? ` · ${l.extra}` : ''}
                      </Text>
                    </span>
                    <LibraryRowMenu
                      url={l.url || undefined}
                      role={l.role}
                      label={l.title}
                      onToggleRole={r => relinkRole(l.id, r)}
                      onUnlink={() => void unlink(l.id, slot.source)}
                      unlinkBusy={busyKey === l.id}
                    />
                  </div>
                ))}
                {slot.source !== 'gameatlas' && (
                  <Button size="small" theme="borderless" onClick={() => setLinkSource(slot.source)}>
                    ＋ 再挂一条{SOURCE_LABEL[slot.source]}…
                  </Button>
                )}
              </>
            )}
          </div>
        )
      })}
      <LibraryLinkModal
        workId={data.work.id}
        workTitle={data.work.title}
        source={linkSource ?? 'gameatlas'}
        visible={linkSource !== null}
        onClose={() => setLinkSource(null)}
        onLinksChanged={reload}
      />
    </div>
  )
}
