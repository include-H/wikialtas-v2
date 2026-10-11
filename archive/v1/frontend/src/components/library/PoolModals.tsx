import { Modal, TreeSelect } from '@douyinfe/semi-ui'
import type { TreeNodeData } from '../../lib/tree'

/** 「建档 / 关联到…」的目标节点选择弹窗（单条或整组共用）。 */
export function PoolPickModal({
  open,
  mode,
  subject,
  count,
  treeData,
  pickParent,
  onPickParentChange,
  onCancel,
  onOk,
}: {
  open: boolean
  mode: 'link' | 'archive'
  subject: string
  count: number
  treeData: TreeNodeData[]
  pickParent: string | undefined
  onPickParentChange: (v: string | undefined) => void
  onCancel: () => void
  onOk: () => void
}) {
  const verb = mode === 'archive' ? '建档' : '关联'
  const what = count > 1 ? `「${subject}」这 ${count} 条` : `「${subject}」`
  return (
    <Modal
      title={`把${what}${verb}到哪个节点下？`}
      visible={open}
      onCancel={onCancel}
      onOk={onOk}
      okText={verb}
    >
      <TreeSelect
        treeData={treeData}
        value={pickParent}
        onChange={(v) => onPickParentChange(v == null ? undefined : String(Array.isArray(v) ? v[0] : v))}
        placeholder="选一个节点（集合或单作都行）"
        style={{ width: '100%' }}
      />
      <div className="dialog-hint">
        不选 = <b>不归属</b>：直接在顶级建节点（之后可以在作品库里把它拖到某个系列下）。
      </div>
    </Modal>
  )
}
