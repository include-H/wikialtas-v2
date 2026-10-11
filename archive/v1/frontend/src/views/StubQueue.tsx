import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Button, Empty, Toast, Typography } from '@douyinfe/semi-ui'
import { createBatchWiki } from '../lib/api'
import { useAppStore } from '../lib/store'
import StubTable from '../components/library/StubTable'

const { Title, Text } = Typography

/**
 * 待写正文——节点已经在库里建好了（「建档」= 建节点 + 挂链），缺的是正文。
 *
 * 从「媒体库建议」搬出来独立成页：那边是"库条目还没挂链"的裁决台（媒体列表），
 * 这边是"节点已建、等 Altas 写"的任务队列（没有封面、也没有库来源）。
 * 两件事挤在一页，就是两边都看不清——这页的字一律说"写"，不再说"建档"。
 */
export default function StubQueue() {
  const nav = useNavigate()
  const { me, nodes, setActiveBatchId, setAiPanelOpen } = useAppStore()
  const [starting, setStarting] = useState(false)
  const stubs = useMemo(() => nodes.filter((n) => n.status === 'stub'), [nodes])

  const start = async (ids: string[], cap: number) => {
    setStarting(true)
    try {
      const batch = ids.slice(0, cap)
      const res = await createBatchWiki({ workIds: batch, batchSize: batch.length })
      setActiveBatchId(res.batchId)
      setAiPanelOpen(true)
      const rest = ids.length - res.runIds.length
      Toast.success(
        rest > 0
          ? `已排入 ${res.runIds.length} 个工单；其余 ${rest} 篇还在列表里，可再点一次`
          : `已排入 ${res.runIds.length} 个工单`,
      )
    } catch (e) {
      Toast.error(e instanceof Error ? e.message : '创建批次失败')
    } finally {
      setStarting(false)
    }
  }

  if (!me.authed) {
    return <Empty description="待写正文需要登录后查看" style={{ margin: 64 }} />
  }

  return (
    <div className="pool-page">
      <div className="pool-inner">
        <div className="pool-head">
          <Title heading={4} style={{ margin: 0 }}>
            待写正文
          </Title>
          <div className="pool-head-btns">
            <Button theme="borderless" type="tertiary" onClick={() => nav('/library')}>
              媒体库建议
            </Button>
          </div>
        </div>
        <Text type="tertiary" size="small">
          节点已经建好、正文还是空的——勾选后交给 Altas 按骨架写（每篇一个工单，进度在右侧 AI 面板）。
        </Text>

        {stubs.length === 0 ? (
          <Empty
            style={{ margin: 48 }}
            description="没有待写的节点——在「媒体库建议」里给库条目建档后，节点会出现在这里"
          />
        ) : (
          <div className="stub-wrap">
            <StubTable stubs={stubs} starting={starting} onStart={(ids, cap) => void start(ids, cap)} />
          </div>
        )}
      </div>
    </div>
  )
}
