import { useAppStore } from '../../lib/store'
import { useWorkLibrary } from '../../lib/useWorkLibrary'
import type {
  NodeLibraryResponse,
  NodePendingItem,
  Relation,
} from '../../types'
import LibrarySlots from './LibrarySlots'
import LibrarySupplements from './LibrarySupplements'
import LibraryPending from './LibraryPending'
import LibraryRelations from './LibraryRelations'

export interface LibraryPanelProps {
  workId: string
  data: NodeLibraryResponse
  relations: Relation[]
  watching: boolean
  busyKey: string | null
  reload: () => void
  linkPending: (s: NodePendingItem) => void
  archivePending: (s: NodePendingItem) => void
  dismissPending: (s: NodePendingItem, reason?: string) => void
  restoreDismissed: (key: string) => void
  rescan: (term?: string) => void
  unlink: (linkId: string, source: 'gameatlas' | 'emby' | 'komga') => void
  relinkRole: (linkId: string, role: 'work' | 'mix') => void
}

/**
 * 右侧栏「关联与媒体」标签(2026-09-16 起合并自「关联」+「媒体」两个标签):
 *
 *   ① 正片位   —— 只渲染介质对应的那个库(电影/剧集→Emby、游戏→GA、书→Komga),
 *                   槽内是 role=work 的链(介质不明时三槽全显)
 *   ② 混杂内容 —— role=mix 的链(OST/解析/预告…),不限条
 *   ③ 待确认   —— 扫库/「AI 对一遍」产出的建议,确认才挂链
 *   ④ 作品关系 —— Altas 随工单建立的关系边(只读)
 *
 * 动态序:待确认有内容时浮到最上(写完看结果的场景);空时落回 ③ 位,
 * 收成一条细条保留「检索媒体库」入口。
 * 数据面单一(useWorkLibrary)——去重按条目 id,无标题启发式。
 */
export default function RelatedMediaPanel({ workId }: { workId: string }) {
  const { me } = useAppStore()
  const lib = useWorkLibrary(workId)
  const { data } = lib

  if (!me.authed) return <div className="lm-panel" />
  if (lib.loading && !data) {
    return (
      <div className="lm-panel">
        <div className="lm-loading">加载关联与媒体…</div>
      </div>
    )
  }
  if (lib.error && !data) {
    return (
      <div className="lm-panel">
        <div className="lm-error">
          {lib.error}
          <a className="lm-retry" onClick={() => void lib.reload()}>
            重试
          </a>
        </div>
      </div>
    )
  }
  if (!data) return <div className="lm-panel" />

  const props: LibraryPanelProps = {
    workId,
    data,
    relations: lib.relations,
    watching: lib.watching,
    busyKey: lib.busyKey,
    reload: lib.reload,
    linkPending: lib.linkPending,
    archivePending: lib.archivePending,
    dismissPending: lib.dismissPending,
    restoreDismissed: lib.restoreDismissed,
    rescan: lib.rescan,
    unlink: lib.unlink,
    relinkRole: lib.relinkRole,
  }
  const pending = data.pending

  return (
    <div className="lm-panel">
      {pending.length > 0 ? (
        <>
          <LibraryPending {...props} />
          <LibrarySlots {...props} />
          <LibrarySupplements {...props} />
          <LibraryRelations {...props} />
        </>
      ) : (
        <>
          <LibrarySlots {...props} />
          <LibrarySupplements {...props} />
          <LibraryPending {...props} />
          <LibraryRelations {...props} />
        </>
      )}
    </div>
  )
}

