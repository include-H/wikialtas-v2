import { useEffect, useState } from 'react'
import { Button, Input, Modal, Tooltip } from 'antd'
import { EditOutlined } from '@ant-design/icons'

/**
 * 说明块（`archive/v1/VISUAL_SPEC.md` §1.4）：正文开头的 `> 说明：…`，
 * 单独渲染成「浅灰底 + 斜体」的说明条，而不是普通引用块。
 *
 * 与题记同理：源码（`>` 那几行）不进编辑器，悬停 ✎ 改原文，保存时接回文档头。
 */
export default function NoteBlock({
  source,
  onChange,
  readOnly,
}: {
  source: string
  onChange: (next: string) => void
  /** 阅读态下不出现编辑入口 */
  readOnly?: boolean
}) {
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState(source)

  useEffect(() => setDraft(source), [source])

  // 显示时去掉 `>` 前缀，拼回成一段话
  const text = source
    .split('\n')
    .map((l) => l.replace(/^>[ \t]?/, '').trimEnd())
    .filter((l) => l.length > 0)
    .join(' ')

  return (
    <div className="doc-note-wrap">
      <div className="doc-note">{text}</div>
      {!readOnly && (
        <Tooltip title="改说明原文">
          <Button
            className="doc-note-edit"
            type="text"
            size="small"
            icon={<EditOutlined />}
            aria-label="编辑说明"
            onClick={() => setOpen(true)}
          />
        </Tooltip>
      )}

      <Modal
        title="说明原文"
        open={open}
        onCancel={() => setOpen(false)}
        onOk={() => {
          onChange(draft.endsWith('\n') ? draft : `${draft}\n`)
          setOpen(false)
        }}
        okText="保存"
        cancelText="取消"
        width={560}
      >
        <Input.TextArea
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          autoSize={{ minRows: 4, maxRows: 12 }}
          spellCheck={false}
          style={{ fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace', fontSize: 13 }}
        />
        <div style={{ marginTop: 8, fontSize: 12, color: 'var(--ant-color-text-tertiary)' }}>
          每行以 `&gt; ` 开头；整理范围、来源分层、剧透范围、检索时间都写在这里。
        </div>
      </Modal>
    </div>
  )
}
