import { useEffect, useState } from 'react'
import { Button, Input, Modal, Tooltip } from 'antd'
import { EditOutlined } from '@ant-design/icons'

import { parseEpigraph } from '../epigraph'

/**
 * 题记（本产品唯一允许「文艺」的组件；`archive/v1/VISUAL_SPEC.md` §3）。
 *
 * 渲染形态 = **书页留白**：居中、衬线、上下细分隔线、一对对角装饰引号（CSS ::before/::after）、
 * 署名右对齐，**无底色、无边框盒子**——严禁卡片化，也不是普通 blockquote。
 *
 * 编辑：源码（`:::epigraph` 围栏）不在正文编辑器里（v1 也是这个做法：编辑态按渲染形态显示），
 * 悬停出现 ✎，点开用一个小弹层改围栏原文。
 */
export default function Epigraph({
  fence,
  onChange,
  readOnly,
}: {
  fence: string
  /** 改完回调新的围栏原文（含 `:::`） */
  onChange: (nextFence: string) => void
  /** 阅读态下不出现编辑入口 */
  readOnly?: boolean
}) {
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState(fence)

  useEffect(() => setDraft(fence), [fence])

  // 收尾的 `:::` 之后可能还跟着空行（splitDoc 会把它们留在围栏里，保证往返不丢），
  // 所以这里要吃掉尾随空白，否则 `:::` 会漏成正文一行、署名也认不出来。
  // 开头同理：`header.fence` 可能带着围栏前的空行（splitDoc 的 fenceGap），
  // 所以剥 `:::epigraph` 要容忍前导空白——不然那一行会原样显示在题记上方。
  const inner = fence
    .replace(/^\s*:::epigraph[^\n]*\r?\n/, '')
    .replace(/\r?\n:::[ \t]*\s*$/, '')
  const { lines, author } = parseEpigraph(inner)

  return (
    <div className="doc-epigraph-wrap">
      <figure className="doc-epigraph">
        <div className="doc-epigraph-content">
          <blockquote className="doc-epigraph-body">
            {lines.map((line, i) =>
              line.type === 'latin' ? (
                <p className="doc-epigraph-line doc-epigraph-line--latin" key={`latin-${i}`}>
                  {line.text}
                </p>
              ) : (
                <p
                  className="doc-epigraph-line doc-epigraph-line--display"
                  key={`display-${i}`}
                  style={{ fontSize: line.style.fontSize, letterSpacing: line.style.letterSpacing }}
                >
                  {line.segments.map((seg, j) => (
                    <span className="doc-epigraph-segment" key={`seg-${j}`}>
                      <span className="doc-epigraph-segment-text">{seg.text}</span>
                      {seg.punctuation && (
                        <span className="doc-epigraph-punctuation">{seg.punctuation}</span>
                      )}
                    </span>
                  ))}
                </p>
              ),
            )}
          </blockquote>
          {author && <figcaption className="doc-epigraph-author">—— {author}</figcaption>}
        </div>

        {!readOnly && (
          <Tooltip title="改题记原文（:::epigraph 围栏）">
            <Button
              className="doc-epigraph-edit"
              type="text"
              size="small"
              icon={<EditOutlined />}
              aria-label="编辑题记"
              onClick={() => setOpen(true)}
            />
          </Tooltip>
        )}
      </figure>

      <Modal
        title="题记原文"
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
          autoSize={{ minRows: 6, maxRows: 16 }}
          spellCheck={false}
          style={{ fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace', fontSize: 13 }}
        />
        <div style={{ marginTop: 8, fontSize: 12, color: 'var(--ant-color-text-tertiary)' }}>
          第一行 `:::epigraph`、最后一行 `:::`；中间一行原文一行译文，署名那行以 `——` 开头。
        </div>
      </Modal>
    </div>
  )
}
