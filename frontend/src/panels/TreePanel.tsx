import { useMemo, useState, type ReactNode } from 'react'
import {
  App as AntApp,
  Badge,
  Button,
  Form,
  Input,
  Modal,
  Radio,
  Select,
  Tooltip,
  Tree,
} from 'antd'
import {
  DatabaseOutlined,
  HomeOutlined,
  MenuFoldOutlined,
  PlusOutlined,
  SafetyCertificateOutlined,
  SearchOutlined,
  SettingOutlined,
} from '@ant-design/icons'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { api, type Medium, type Node, type NodeKind } from '../api'
import { KIND_LABEL, MEDIUM_LABEL, nodeIcon, statusClass } from '../nodeVisual'
import { useUi } from '../store'

interface RawNode {
  key: string
  title: ReactNode
  children: RawNode[]
}

/** 后端给扁平列表（父在前、同级按 sort_key）；这里拼成树 + 做本地过滤。 */
function buildTree(nodes: Node[], keyword: string): RawNode[] {
  const kw = keyword.trim().toLowerCase()
  const matched = kw
    ? new Set(nodes.filter((n) => n.title.toLowerCase().includes(kw)).map((n) => n.id))
    : null
  // 过滤时保留命中节点的祖先链
  const keep = new Set<string>()
  if (matched) {
    const byId = new Map(nodes.map((n) => [n.id, n]))
    for (const id of matched) {
      let cur: Node | undefined = byId.get(id)
      while (cur) {
        keep.add(cur.id)
        cur = cur.parent_id ? byId.get(cur.parent_id) : undefined
      }
    }
  }
  const childrenOf = new Map<string | null, Node[]>()
  for (const n of nodes) {
    if (keep.size > 0 && !keep.has(n.id)) continue
    const bucket = childrenOf.get(n.parent_id)
    if (bucket) bucket.push(n)
    else childrenOf.set(n.parent_id, [n])
  }
  const build = (parent: string | null): RawNode[] =>
    (childrenOf.get(parent) ?? []).map((n) => ({
      key: n.id,
      title: (
        <span className={`tree-node-title ${statusClass(n.status)}`}>
          <span className="tree-node-icon">{nodeIcon(n)}</span>
          <span>{n.title}</span>
        </span>
      ),
      children: build(n.id),
    }))
  return build(null)
}

export default function TreePanel({
  onNavigate,
  onCollapse,
}: {
  onNavigate?: () => void
  onCollapse?: () => void
} = {}) {
  const qc = useQueryClient()
  const { message } = AntApp.useApp()
  const { selectedId, select, view, setView } = useUi()
  const [open, setOpen] = useState(false)
  const [keyword, setKeyword] = useState('')
  const [form] = Form.useForm()
  const kind: NodeKind = Form.useWatch('kind', form) ?? 'work'

  const nodes = useQuery({ queryKey: ['tree'], queryFn: api.tree })
  const archived = useQuery({ queryKey: ['archived'], queryFn: api.archived })
  const overview = useQuery({ queryKey: ['overview'], queryFn: api.overview })

  const create = useMutation({
    mutationFn: api.createNode,
    onSuccess: (n) => {
      setOpen(false)
      form.resetFields()
      qc.invalidateQueries({ queryKey: ['tree'] })
      select(n.id)
      message.success(`已建：${n.title}`)
    },
    onError: (e) => message.error(e.message),
  })

  const treeData = useMemo(() => buildTree(nodes.data ?? [], keyword), [nodes.data, keyword])
  const total = nodes.data?.length ?? 0

  return (
    <div className="side-nav">
      <div className="side-brand">
        <span className="brand-mark">A</span>
        <span>Atlas</span>
      </div>

      <Input
        className="side-search"
        prefix={<SearchOutlined style={{ color: 'var(--ant-color-text-tertiary)' }} />}
        placeholder="搜索"
        allowClear
        value={keyword}
        onChange={(e) => setKeyword(e.target.value)}
      />

      {/* v1 的全局导航；主页 = 未选节点时的首页（`design.md §6.1 §6.2`） */}
      <nav className="side-links">
        <div
          className={`side-link${view === 'doc' && !selectedId ? ' is-active' : ''}`}
          onClick={() => {
            select(null)
            setView('doc')
            onNavigate?.()
          }}
        >
          <HomeOutlined />
          <span>主页</span>
        </div>
        <div
          className={`side-link${view === 'pool' ? ' is-active' : ''}`}
          onClick={() => {
            setView('pool')
            onNavigate?.()
          }}
        >
          <DatabaseOutlined />
          <span>媒体库池子</span>
          <span style={{ flex: 1 }} />
          {overview.data && overview.data.unbuilt > 0 && (
            <Badge count={overview.data.unbuilt} overflowCount={9999} color="var(--ant-color-primary)" />
          )}
        </div>
        <div
          className={`side-link${view === 'search' ? ' is-active' : ''}`}
          onClick={() => {
            setView('search')
            onNavigate?.()
          }}
        >
          <SearchOutlined />
          <span>全文检索</span>
        </div>
        <div
          className={`side-link${view === 'audit' ? ' is-active' : ''}`}
          onClick={() => {
            setView('audit')
            onNavigate?.()
          }}
        >
          <SafetyCertificateOutlined />
          <span>体检与 Review</span>
        </div>
        <div
          className={`side-link${view === 'settings' ? ' is-active' : ''}`}
          onClick={() => {
            setView('settings')
            onNavigate?.()
          }}
        >
          <SettingOutlined />
          <span>设置与用量</span>
        </div>
      </nav>

      <div className="side-section-head" style={{ display: 'flex', alignItems: 'center' }}>
        <span>我的文档库</span>
        <span style={{ flex: 1 }} />
        <Tooltip title="新建节点">
          <Button
            type="text"
            size="small"
            icon={<PlusOutlined />}
            onClick={() => setOpen(true)}
            aria-label="新建节点"
          />
        </Tooltip>
      </div>

      <div className="side-tree-scroll">
        {nodes.isLoading ? (
          <div style={{ padding: '4px 8px', fontSize: 12, color: 'var(--ant-color-text-tertiary)' }}>
            载入中…
          </div>
        ) : treeData.length === 0 ? (
          <div style={{ padding: '4px 8px', fontSize: 12, color: 'var(--ant-color-text-tertiary)' }}>
            {keyword ? '没有匹配的节点' : '还空着，点右上角 + 建一个'}
          </div>
        ) : (
          <Tree
            blockNode
            treeData={treeData}
            defaultExpandAll
            selectedKeys={selectedId ? [selectedId] : []}
            onSelect={(keys) => {
              // antd 的 Tree 里点「已选中」的节点会反选，onSelect 给的是空数组。
              // 那不该把文档清空——空选时保持当前节点。
              const id = keys[0] as string | undefined
              if (id) {
                select(id)
                setView('doc')
              }
              // 窄屏时树在抽屉里：无论是不是反选，都让位给正文
              onNavigate?.()
            }}
          />
        )}
      </div>

      {!!archived.data?.length && (
        <details className="side-archived">
          <summary>已归档（{archived.data.length}）</summary>
          {archived.data.map((n) => (
            <div
              key={n.id}
              className={`side-archived-row${selectedId === n.id ? ' is-active' : ''}`}
              onClick={() => {
                select(n.id)
                setView('doc')
                onNavigate?.()
              }}
            >
              {n.title}
            </div>
          ))}
        </details>
      )}

      <div className="side-foot">
        <span>{total} 个节点</span>
        {onCollapse && (
          <Tooltip title="收起知识库">
            <Button
              type="text"
              size="small"
              aria-label="收起知识库"
              icon={<MenuFoldOutlined />}
              onClick={onCollapse}
            />
          </Tooltip>
        )}
      </div>

      <Modal
        title="新建节点"
        open={open}
        onCancel={() => setOpen(false)}
        onOk={() => form.submit()}
        confirmLoading={create.isPending}
        destroyOnHidden
      >
        <Form
          form={form}
          layout="vertical"
          initialValues={{ kind: 'work', medium: 'game' }}
          onFinish={(v) =>
            create.mutate({
              parent_id: v.parent_id ?? null,
              kind: v.kind,
              medium: v.kind === 'doc' ? null : (v.medium ?? null),
              title: v.title,
            })
          }
        >
          <Form.Item name="kind" label="类型" rules={[{ required: true }]}>
            <Radio.Group>
              {(Object.keys(KIND_LABEL) as NodeKind[]).map((k) => (
                <Radio.Button key={k} value={k}>
                  {KIND_LABEL[k]}
                </Radio.Button>
              ))}
            </Radio.Group>
          </Form.Item>

          <Form.Item name="parent_id" label="挂在">
            <Select
              allowClear
              placeholder="（根）"
              options={(nodes.data ?? [])
                .filter((n) => n.kind !== 'doc')
                .map((n) => ({ value: n.id, label: `${n.title}（${KIND_LABEL[n.kind]}）` }))}
            />
          </Form.Item>

          <Form.Item name="title" label="标题" rules={[{ required: true, message: '标题不能为空' }]}>
            <Input placeholder="例：空之轨迹 SC" />
          </Form.Item>

          {kind !== 'doc' && (
            <Form.Item name="medium" label="介质" rules={[{ required: kind === 'work' }]}>
              <Select
                allowClear={kind !== 'work'}
                options={(Object.keys(MEDIUM_LABEL) as Medium[]).map((m) => ({
                  value: m,
                  label: MEDIUM_LABEL[m],
                }))}
              />
            </Form.Item>
          )}
          {kind === 'doc' && (
            <div style={{ color: 'var(--ant-color-text-tertiary)', fontSize: 12 }}>
              资料（doc）不带介质——这是落位规则，库里也有 CHECK 挡着。
            </div>
          )}
        </Form>
      </Modal>
    </div>
  )
}
