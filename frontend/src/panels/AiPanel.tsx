import { Button, Typography } from 'antd'
import { PlusOutlined, ThunderboltOutlined } from '@ant-design/icons'
import { useQuery } from '@tanstack/react-query'

import { api } from '../api'
import { useUi } from '../store'

/**
 * 右栏 = Altas（v1 的 480px 标签页侧栏，`archive/v1/VISUAL_SPEC.md` §1.5）。
 *
 * 目前只有壳：头部（Altas + 当前文档）、空态、底部输入框。
 * 真接上模型是尖刺 B——那时候 `response.*` 交给 AntD X 的 runtime，宿主事件走自己的部件。
 */
export default function AiPanel() {
  const selectedId = useUi((s) => s.selectedId)
  const tree = useQuery({ queryKey: ['tree'], queryFn: api.tree })
  const context = (tree.data ?? []).find((n) => n.id === selectedId)?.title ?? ''

  return (
    <div className="ai-panel">
      <div className="ai-head">
        <span className="ai-mark">
          <ThunderboltOutlined />
        </span>
        <span className="ai-title">Altas</span>
        {context && <span className="ai-context">{context}</span>}
        <span style={{ flex: 1 }} />
        <Button type="text" size="small" icon={<PlusOutlined />} disabled aria-label="新对话" />
      </div>

      <div className="ai-body">
        <div className="ai-hint">这一轮先把「文档编辑 + 存储」这条路打通</div>
        <div className="ai-empty">
          <Typography.Text type="secondary">今天有什么工作需要处理？</Typography.Text>
          <div style={{ marginTop: 6, fontSize: 12, color: 'var(--ant-color-text-quaternary)' }}>
            问它要资料、让它写正文、让它改某一节——都由它自己判断
          </div>
        </div>
      </div>

      <div className="ai-foot">
        <div
          style={{
            border: '1px solid var(--ant-color-border)',
            borderRadius: 8,
            padding: '8px 10px',
            fontSize: 13,
            color: 'var(--ant-color-text-quaternary)',
            background: 'var(--ant-color-fill-quaternary)',
          }}
        >
          发消息或创建任务… 使用技能 @ 添加资料
        </div>
      </div>
    </div>
  )
}
