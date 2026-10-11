import { App as AntApp, Button, List, Modal, Tag } from 'antd'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { api } from '../../api'

/** 「已忽略」的恢复入口（`docs/library-sync.md` §4：忽略都可恢复）。 */
export default function IgnoresModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const qc = useQueryClient()
  const { message } = AntApp.useApp()
  const ignores = useQuery({ queryKey: ['library-ignores'], queryFn: api.libraryIgnores, enabled: open })

  const restore = useMutation({
    mutationFn: (id: string) => api.removeIgnore(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['library-pool'] })
      qc.invalidateQueries({ queryKey: ['library-ignores'] })
      message.success('已恢复')
    },
    onError: (e: Error) => message.error(e.message),
  })

  return (
    <Modal title="已忽略的条目 / 组" open={open} onCancel={onClose} footer={null} destroyOnHidden>
      <List
        size="small"
        loading={ignores.isLoading}
        dataSource={ignores.data ?? []}
        locale={{ emptyText: '没有忽略任何条目' }}
        renderItem={(i) => (
          <List.Item
            actions={[
              <Button key="r" type="link" size="small" onClick={() => restore.mutate(i.id)}>
                恢复
              </Button>,
            ]}
          >
            <span>
              <Tag>{i.source ?? '全部源'}</Tag>
              {i.external_id ? `条目 ${i.external_id}` : `整组 ${i.group_key}`}
              {i.reason ? ` · ${i.reason}` : ''}
            </span>
          </List.Item>
        )}
      />
    </Modal>
  )
}
