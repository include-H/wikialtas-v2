import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Button, Input, Select, Table, Typography } from '@douyinfe/semi-ui'
import type { ColumnProps, RowSelectionProps } from '@douyinfe/semi-ui/lib/es/table/interface'
import { IconAIFilledLevel1, IconSearch } from '@douyinfe/semi-icons'
import type { WorkSummary } from '../../types'
import { workPath } from '../../lib/routes'
import { ancestorPath, relativeTime } from '../../lib/tree'
import { MEDIUM_LABEL } from '../../lib/mediaTags'

const { Text } = Typography

/** 一次最多起几篇（后端 batchMaxSize=20 会夹住，这里先摆明白）。 */
const CAPS = [
  { value: '5', label: '本次最多 5 篇' },
  { value: '10', label: '本次最多 10 篇' },
  { value: '20', label: '本次最多 20 篇' },
]

/**
 * 待写正文的表：节点已经建好（archive 干完了），缺的是正文。
 * 勾选批量——**勾谁写谁**，不再有"取前 N 条"那种暗规则。
 */
export default function StubTable({
  stubs,
  starting,
  onStart,
}: {
  stubs: WorkSummary[]
  starting: boolean
  onStart: (ids: string[], cap: number) => void
}) {
  const nav = useNavigate()
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState<string[]>([])
  const [cap, setCap] = useState('5')

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase()
    return q ? stubs.filter((s) => s.title.toLowerCase().includes(q)) : stubs
  }, [stubs, query])

  const columns = useMemo<ColumnProps<WorkSummary>[]>(
    () => [
      {
        title: '节点',
        dataIndex: 'title',
        render: (_: unknown, n: WorkSummary) => (
          <span className="stub-cell">
            <span className="stub-cell-title" title={n.title}>
              {n.title}
            </span>
            <Text type="tertiary" size="small" className="stub-cell-path">
              {ancestorPath(stubs, n.id)
                .slice(0, -1)
                .map((p) => p.title)
                .join(' / ') || '顶级'}
            </Text>
          </span>
        ),
      },
      {
        title: '介质',
        width: 90,
        render: (_: unknown, n: WorkSummary) => (
          <Text type="tertiary" size="small">
            {MEDIUM_LABEL[n.medium ?? ''] ?? n.medium ?? '—'}
          </Text>
        ),
      },
      {
        title: '建立于',
        width: 110,
        render: (_: unknown, n: WorkSummary) => (
          <Text type="tertiary" size="small">
            {relativeTime(n.updatedAt)}
          </Text>
        ),
      },
      {
        title: '',
        width: 80,
        render: (_: unknown, n: WorkSummary) => (
          <Button size="small" theme="borderless" type="primary" onClick={() => nav(workPath(n.id))}>
            打开
          </Button>
        ),
      },
    ],
    [stubs, nav],
  )

  const rowSelection = useMemo<RowSelectionProps<WorkSummary>>(
    () => ({
      selectedRowKeys: selected,
      onChange: (keys) => setSelected(keys as string[]),
    }),
    [selected],
  )

  return (
    <>
      <div className="stub-bulk">
        <Input
          className="stub-bulk-search"
          prefix={<IconSearch />}
          placeholder="搜索标题…"
          showClear
          value={query}
          onChange={setQuery}
        />
        <Text type="tertiary" size="small">
          {selected.length > 0 ? `已勾选 ${selected.length} 篇` : `勾选想写的节点（共 ${stubs.length} 篇）`}
        </Text>
        <div className="stub-bulk-actions">
          <Select<string>
            size="small"
            value={cap}
            onChange={(v) => setCap(String(v))}
            optionList={CAPS}
            style={{ width: 152 }}
          />
          <Button
            size="small"
            theme="solid"
            className="ai-coedit-btn"
            icon={<IconAIFilledLevel1 />}
            loading={starting}
            disabled={selected.length === 0}
            onClick={() => onStart(selected, Number(cap))}
          >
            交给 Altas 写（{selected.length} 篇）
          </Button>
        </div>
      </div>
      <Table
        rowKey="id"
        columns={columns}
        dataSource={shown}
        rowSelection={rowSelection}
        pagination={false}
        size="small"
        scroll={{ y: 460 }}
      />
    </>
  )
}
