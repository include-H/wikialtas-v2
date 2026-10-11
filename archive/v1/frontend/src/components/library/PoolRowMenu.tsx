import { Button, Dropdown } from '@douyinfe/semi-ui'
import { IconMore } from '@douyinfe/semi-icons'
import type { PoolEntry, WorkSummary } from '../../types'

/**
 * 砖块/行的行尾「⋯」：把操作收进下拉，砖块上只留勾选状态与角标。
 *
 * 有 `node`（Wiki 已有同名/别名节点）时第一项是「挂到《X》」——那是"不用建档"的路径；
 * 没有就给「建档为待写…」。「建档到别的节点…」始终留着（有时你想挂到上层系列）。
 */
export default function PoolRowMenu({
  entry,
  node,
  onLink,
  onArchive,
  onIgnore,
}: {
  entry: PoolEntry
  node?: WorkSummary
  onLink: (e: PoolEntry, node: WorkSummary) => void
  onArchive: (e: PoolEntry) => void
  onIgnore: (e: PoolEntry) => void
}) {
  return (
    <Dropdown
      position="bottomRight"
      trigger="click"
      render={
        <Dropdown.Menu>
          {node ? (
            <Dropdown.Item onClick={() => onLink(entry, node)}>挂到《{node.title}》</Dropdown.Item>
          ) : (
            <Dropdown.Item onClick={() => onArchive(entry)}>建档为待写…</Dropdown.Item>
          )}
          {node ? <Dropdown.Item onClick={() => onArchive(entry)}>建档到别的节点…</Dropdown.Item> : null}
          {entry.url ? (
            <Dropdown.Item onClick={() => window.open(entry.url, '_blank', 'noreferrer')}>
              在库中打开
            </Dropdown.Item>
          ) : null}
          <Dropdown.Divider />
          <Dropdown.Item type="danger" onClick={() => onIgnore(entry)}>
            忽略这条
          </Dropdown.Item>
        </Dropdown.Menu>
      }
    >
      <Button
        theme="borderless"
        type="tertiary"
        size="small"
        icon={<IconMore />}
        className="lib-row-more"
        title={`更多操作：${entry.title}`}
        onClick={(e) => e.stopPropagation()}
      />
    </Dropdown>
  )
}
