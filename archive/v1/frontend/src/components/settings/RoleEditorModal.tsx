import { useEffect, useMemo, useState } from 'react'
import { Button, Modal, Select, Table } from '@douyinfe/semi-ui'
import type { ColumnProps, RowSelectionProps } from '@douyinfe/semi-ui/lib/es/table/interface'
import type { EmbyLibraryView } from '../../types'
import { Text, type EmbyRole } from './common'

/** 弹窗行类型（EmbyLibraryView 的别名引用，rowKey=id） */
type Row = EmbyLibraryView

const ROLE_OPTIONS = [
  { value: '', label: '不参与' },
  { value: 'work', label: '正片（建档建议）' },
  { value: 'mixed', label: '混杂内容（关联）' },
]

interface Props {
  open: boolean
  views: EmbyLibraryView[]
  initial: Record<string, EmbyRole>
  /** 全局保存进行中：确认按钮 loading（声明式 Modal 不会自动关窗，宿主成功后自行 close） */
  committing: boolean
  onCancel: () => void
  onCommit: (roles: Record<string, EmbyRole>) => Promise<unknown>
}

/**
 * Emby 媒体库角色编辑：表检出的是「当前已存角色」，弹窗内所有改动只落在
 * draft 上——取消 = 丢弃，保存 = 提交全局 putSettings（由宿主完成）。
 * 勾选多行后可用顶部按钮批量设角色（如「所有 tvshows 设为正片」）。
 */
export function RoleEditorModal({ open, views, initial, committing, onCancel, onCommit }: Props) {
  const [draft, setDraft] = useState<Record<string, EmbyRole>>({})
  const [selected, setSelected] = useState<string[]>([])

  // 每次打开从 initial 重置草稿与勾选
  useEffect(() => {
    if (open) {
      setDraft({ ...initial })
      setSelected([])
    }
  }, [open, initial])

  const dataSource = useMemo(() => views, [views])

  const columns = useMemo<ColumnProps<Row>[]>(
    () => [
      { title: '库', dataIndex: 'name', render: (t: string) => <span title={t}>{t}</span> },
      {
        title: '类型',
        dataIndex: 'collectionType',
        width: 110,
        render: (t?: string) => <Text type="tertiary" size="small">{t || '—'}</Text>,
      },
      {
        title: '角色',
        width: 200,
        render: (_: unknown, record: Row) => (
          <Select<string>
            size="small"
            value={draft[record.id] ?? ''}
            onChange={(v) => setDraft((prev) => ({ ...prev, [record.id]: String(v ?? '') as EmbyRole }))}
            optionList={ROLE_OPTIONS}
            style={{ width: 170 }}
          />
        ),
      },
    ],
    [draft],
  )

  const rowSelection = useMemo<RowSelectionProps<Row>>(
    () => ({
      selectedRowKeys: selected,
      onChange: (keys) => setSelected(keys as string[]),
    }),
    [selected],
  )

  const applyToSelected = (role: EmbyRole) => {
    setDraft((prev) => {
      const next = { ...prev }
      for (const id of selected) next[id] = role
      return next
    })
    setSelected([])
  }

  return (
    <Modal
      title="编辑 Emby 媒体库角色"
      visible={open}
      width={720}
      okText="保存"
      cancelText="取消"
      okButtonProps={{ loading: committing }}
      onOk={() => onCommit({ ...draft })}
      onCancel={onCancel}
      maskClosable={false}
    >
      <Text type="tertiary" size="small">
        正片（Work）：集合页给「建议建档」；混杂内容（Mixed）：节点页给「关联候选」（第九艺术/解析这类）。
        保存后立刻对建议池与下次扫库生效。
      </Text>
      <div className="role-modal-bulk">
        <Text type="tertiary" size="small">
          {selected.length > 0 ? `已勾选 ${selected.length} 个库` : '勾选左侧复选框可批量设置角色'}
        </Text>
        <div className="role-modal-bulk-actions">
          <Button size="small" disabled={selected.length === 0} onClick={() => applyToSelected('work')}>
            设为正片
          </Button>
          <Button size="small" disabled={selected.length === 0} onClick={() => applyToSelected('mixed')}>
            设为混杂内容
          </Button>
          <Button size="small" disabled={selected.length === 0} onClick={() => applyToSelected('')}>
            设为不参与
          </Button>
        </div>
      </div>
      <Table
        rowKey="id"
        columns={columns}
        dataSource={dataSource}
        rowSelection={rowSelection}
        pagination={false}
        size="small"
        scroll={{ y: 430 }}
      />
    </Modal>
  )
}
