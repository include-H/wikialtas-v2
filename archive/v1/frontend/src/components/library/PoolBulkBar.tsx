import { Button, Typography } from '@douyinfe/semi-ui'

const { Text } = Typography

/**
 * 选中 ≥1 时底部浮出的批量条。
 *
 * 「建为待写」要选一个节点（跨容器一选用同一个父级）；「挂上 Wiki 已有的」是把选中里
 * 命中已有节点的那些直接挂上去（不用建档）；「忽略」是记账（这页的主要动作之一，
 * 库里 1400+ 条，噪声要能一把划掉）。
 */
export default function PoolBulkBar({
  count,
  containers,
  hitCount,
  busy,
  onArchive,
  onLinkExisting,
  onIgnore,
  onClear,
}: {
  count: number
  containers: number
  hitCount: number
  busy: string | null
  onArchive: () => void
  onLinkExisting: () => void
  onIgnore: () => void
  onClear: () => void
}) {
  if (count === 0) return null
  return (
    <div className="pool-bulkbar">
      <Text size="small">
        已选 <b>{count}</b> 条{containers > 1 ? `（分布在 ${containers} 个容器）` : ''}
      </Text>
      <div className="pool-bulkbar-actions">
        {hitCount > 0 ? (
          <Button
            size="small"
            theme="borderless"
            type="primary"
            loading={busy === 'bulk:link'}
            onClick={onLinkExisting}
          >
            挂上 Wiki 已有的 {hitCount} 条
          </Button>
        ) : null}
        <Button
          size="small"
          theme="solid"
          type="primary"
          loading={busy?.startsWith('bulk:')}
          onClick={onArchive}
          title="选一个节点，把选中的都建到它下面（一次最多 20 篇）"
        >
          建为待写…
        </Button>
        <Button size="small" theme="borderless" type="danger" onClick={onIgnore}>
          忽略
        </Button>
        <Button size="small" theme="borderless" type="tertiary" onClick={onClear}>
          取消选择
        </Button>
      </div>
    </div>
  )
}
