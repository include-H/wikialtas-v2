import { useEffect } from 'react'
import { useParams } from 'react-router-dom'
import { Typography } from '@douyinfe/semi-ui'
import { IconAIFilledLevel1, IconChevronRight, IconLink } from '@douyinfe/semi-icons'
import { useAppStore, type RightTab } from '../../lib/store'
import { UNKNOWN_WORK_ID } from '../../lib/routes'
import AiPanel from './AiPanel'
import RelatedMediaPanel from '../library/RelatedMediaPanel'

const { Text } = Typography

/**
 * 右侧边栏:LLM 模块(Altas)与「关联与媒体」共用一栏 480px,标签分开(宪法 §1.5)。
 *
 * - 标签:Altas(全页面) + 关联与媒体(仅作品页:正片位 / 混杂 / 待确认 / 作品关系);
 * - 展开态:顶部标签条 + 右侧收起 chevron;
 * - 收起态:右缘 44px 图标拉门(AI + 在途计数,作品页加「关联与媒体」一枚,
 *   角标 = 待确认条数);
 * - 面板**常驻挂载**(切标签/收起只是 CSS 隐藏)——在途工单的 SSE 与流式不断线;
 * - 收放状态与当前标签记 localStorage,默认收起。
 */
export default function RightSidebar() {
  const params = useParams()
  const {
    nodes,
    aiPanelOpen,
    setAiPanelOpen,
    rightTab,
    setRightTab,
    openRightSide,
    activeRuns,
    pendingCount,
  } = useAppStore()

  const routeId = params.id
  const workId = routeId && routeId !== UNKNOWN_WORK_ID ? routeId : null
  const currentNode = nodes.find((n) => n.id === workId)
  const workTitle = currentNode?.title
  const docId = params.docId ?? null

  // 媒体关联只属于正文节点(work);series/universe 是聚合层,右栏只留 Altas。
  // nodes 尚未加载完时 currentNode 为空 → 暂不显示,加载完自然出现。
  const isBody = !!workId && currentNode?.kind === 'work'
  // 非正文页 关联与媒体 无处可去——渲染恒落 ai(状态在下面同步纠正)
  const tab: RightTab = isBody && rightTab !== 'ai' ? rightTab : 'ai'

  useEffect(() => {
    if (!isBody && rightTab !== 'ai') setRightTab('ai')
  }, [isBody, rightTab, setRightTab])

  const tabs: { key: RightTab; label: string }[] = [
    { key: 'ai', label: 'Altas' },
    ...(isBody ? [{ key: 'library' as RightTab, label: '关联与媒体' }] : []),
  ]

  return (
    <div className={`right-side${aiPanelOpen ? ' is-open' : ''}`}>
      <div
        className={`right-side-rail${aiPanelOpen ? ' is-hidden' : ''}`}
        role="toolbar"
        aria-label="右侧栏"
      >
        <button
          type="button"
          className="right-side-rail-btn is-ai"
          title="展开 Altas"
          aria-label="展开 Altas"
          onClick={() => openRightSide('ai')}
        >
          <IconAIFilledLevel1 />
          {activeRuns.length > 0 && (
            <span className="right-side-rail-badge">{activeRuns.length}</span>
          )}
        </button>
        {isBody && (
          <button
            type="button"
            className="right-side-rail-btn"
            title="展开 关联与媒体"
            aria-label="展开 关联与媒体"
            onClick={() => openRightSide('library')}
          >
            <IconLink />
            {pendingCount > 0 && (
              <span className="right-side-rail-badge">{pendingCount}</span>
            )}
          </button>
        )}
      </div>

      <div className={`right-side-main${aiPanelOpen ? '' : ' is-hidden'}`}>
        <div className="right-side-chrome">
          {isBody ? (
            <div className="right-side-tabs" role="tablist" aria-label="右侧栏标签">
              {tabs.map((t) => (
                <button
                  key={t.key}
                  type="button"
                  role="tab"
                  aria-selected={tab === t.key}
                  className={`right-side-tab${tab === t.key ? ' is-active' : ''}${t.key === 'ai' ? ' is-ai' : ''}`}
                  onClick={() => setRightTab(t.key)}
                >
                  {t.key === 'ai' ? (
                    <span className="right-side-tab-ai">
                      <IconAIFilledLevel1 />
                    </span>
                  ) : (
                    <IconLink />
                  )}
                  {t.label}
                </button>
              ))}
            </div>
          ) : (
            <Text className="right-side-label" strong>
              Altas
            </Text>
          )}
          <button
            type="button"
            className="right-side-collapse"
            aria-label="收起侧栏"
            onClick={() => setAiPanelOpen(false)}
          >
            <IconChevronRight />
          </button>
        </div>
        <div className="right-side-pane">
          <div className={`right-side-tabpane${tab === 'ai' ? ' is-active' : ''}`}>
            <AiPanel workId={workId} workTitle={workTitle} docId={docId} />
          </div>
          {isBody && (
            <div className={`right-side-tabpane${tab === 'library' ? ' is-active' : ''}`}>
              <RelatedMediaPanel workId={workId!} />
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
