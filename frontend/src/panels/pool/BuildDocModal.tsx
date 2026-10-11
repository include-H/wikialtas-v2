import { useEffect } from 'react'
import { App as AntApp, Form, Modal, Radio, Select } from 'antd'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { api, type BuildLinkReq, type LibrarySource, type PoolEntry } from '../../api'
import { useUi } from '../../store'

/**
 * 建档弹窗（`docs/library-sync.md` §2.3）：把一条库条目变成一个文档节点 + 一条链。
 *
 * 落位规则**在后端**（`domain::place`）——这里只让人**覆盖**两件事：
 * 挂到哪个父节点（默认「按容器自动」）、这一条算正片位还是混杂位。
 */
export default function BuildDocModal({
  entry,
  open,
  onClose,
}: {
  entry: PoolEntry | null
  open: boolean
  onClose: () => void
}) {
  const qc = useQueryClient()
  const { message } = AntApp.useApp()
  const select = useUi((s) => s.select)
  const setView = useUi((s) => s.setView)
  const [form] = Form.useForm()

  useEffect(() => {
    if (open) form.resetFields()
  }, [open, form])

  const tree = useQuery({ queryKey: ['tree'], queryFn: api.tree })

  const build = useMutation({
    mutationFn: (req: BuildLinkReq) => api.buildLink(req),
    onSuccess: (out) => {
      qc.invalidateQueries({ queryKey: ['tree'] })
      qc.invalidateQueries({ queryKey: ['library-pool'] })
      message.success(out.reused ? `已挂到已有节点：${out.node.title}` : `已建档：${out.node.title}`)
      onClose()
      setView('doc')
      select(out.node.id)
    },
    onError: (e: Error) => message.error(e.message),
  })

  if (!entry) return null
  const e = entry.entry

  return (
    <Modal
      title="建档"
      open={open}
      onCancel={onClose}
      onOk={() => form.submit()}
      confirmLoading={build.isPending}
      destroyOnHidden
      okText="建档并打开"
    >
      <div className="build-doc-head">
        <strong>{e.title}</strong>
        <span>
          {e.container_title} · {e.kind}
          {e.format ? ` / ${e.format}` : ''}
        </span>
      </div>
      <Form
        form={form}
        layout="vertical"
        initialValues={{ parent_id: null, role: entry.role }}
        onFinish={(v) =>
          build.mutate({
            source: e.source as LibrarySource,
            external_id: e.external_id,
            role: v.role,
            parent_id: v.parent_id ?? null,
          })
        }
      >
        <Form.Item
          name="parent_id"
          label="挂在"
          extra="默认「按容器自动」：作品系列下建/复用系列节点；Emby 的库容器建在顶级。"
        >
          <Select
            allowClear
            placeholder="（按容器自动）"
            options={(tree.data ?? [])
              .filter((n) => n.kind !== 'doc')
              .map((n) => ({ value: n.id, label: n.title }))}
          />
        </Form.Item>
        <Form.Item name="role" label="链的位置" rules={[{ required: true }]}>
          <Radio.Group>
            <Radio.Button value="work">正片位</Radio.Button>
            <Radio.Button value="mix">混杂位</Radio.Button>
          </Radio.Group>
        </Form.Item>
      </Form>
    </Modal>
  )
}
