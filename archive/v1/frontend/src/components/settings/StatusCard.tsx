import { Card, Collapse, Descriptions } from '@douyinfe/semi-ui'
import type { RuntimeInfo } from '../../types'

/** 运行状态：只读仪表盘。stats 用键值对，能力清单收进默认收起的折叠面板。 */
export function StatusCard({ runtime }: { runtime: RuntimeInfo | null }) {
  const data = [
    { key: '模型', value: runtime?.model || '—' },
    { key: '并发上限', value: String(runtime?.maxConcurrentRuns ?? '—') },
    { key: '节点', value: String(runtime?.stats.works ?? '—') },
    { key: '资料', value: String(runtime?.stats.docs ?? '—') },
    { key: '正文版本', value: String(runtime?.stats.revisions ?? '—') },
    {
      key: '工单',
      value: runtime ? `${runtime.stats.runs}（运行中 ${runtime.stats.runningNow ?? 0}）` : '—',
    },
    { key: '运行事件', value: String(runtime?.stats.runEvents ?? '—') },
  ]
  const features = runtime?.features ?? []
  return (
    <Card title="运行状态" className="settings-card">
      <Descriptions layout="horizontal" align="plain" column={4} data={data} />
      {features.length > 0 && (
        <Collapse className="settings-features-collapse">
          <Collapse.Panel header="已实现能力" itemKey="features">
            <ul className="settings-features">
              {features.map((f) => (
                <li key={f}>{f}</li>
              ))}
            </ul>
          </Collapse.Panel>
        </Collapse>
      )}
    </Card>
  )
}
