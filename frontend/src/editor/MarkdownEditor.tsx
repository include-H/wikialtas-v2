import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Milkdown, MilkdownProvider, useEditor } from '@milkdown/react'
import { Editor, defaultValueCtx, editorViewCtx, editorViewOptionsCtx, parserCtx, rootCtx, schemaCtx } from '@milkdown/kit/core'
import { commonmark } from '@milkdown/kit/preset/commonmark'
import { gfm } from '@milkdown/kit/preset/gfm'
import { clipboard } from '@milkdown/kit/plugin/clipboard'
import { history } from '@milkdown/kit/plugin/history'
import { listener, listenerCtx } from '@milkdown/kit/plugin/listener'
import { cursor } from '@milkdown/kit/plugin/cursor'
import { indent } from '@milkdown/kit/plugin/indent'
import { trailing } from '@milkdown/kit/plugin/trailing'
import { block, blockSpec, BlockProvider } from '@milkdown/kit/plugin/block'
import { slashFactory, SlashProvider } from '@milkdown/kit/plugin/slash'
import { tooltipFactory, TooltipProvider } from '@milkdown/kit/plugin/tooltip'
import { $prose } from '@milkdown/kit/utils'
import { Plugin } from '@milkdown/kit/prose/state'
import { DOMParser, DOMSerializer } from '@milkdown/kit/prose/model'

import { bumpEditor, useEditorTick } from './tick'
import { slashQuery } from './SlashMenu'
import Toolbar from './Toolbar'
import SlashMenu from './SlashMenu'
import BlockHandle from './BlockHandle'

import './editor.css'

interface Props {
  initial: string
  onChange: (markdown: string) => void
  readOnly?: boolean
  /**
   * 粘贴纯文本时先让宿主过一道（例如把「文档头」——主标题 / 说明块 / 题记——摘出去，
   * 那几块由文档面板的组件渲染，不进编辑器）。返回要真正插入的 markdown；
   * **原样返回 = 不插手**，交回默认粘贴。
   */
  transformPastedText?: (text: string) => string
  /** 空文档里那句空提示的「AI 帮我写」入口。 */
  onAiWrite?: () => void
}

type PasteTransform = (text: string) => string

const [toolbarSpec, toolbarPlugin] = tooltipFactory('WIKI_TOOLBAR')
const [slashSpec, slashPlugin] = slashFactory('WIKI_SLASH')

/** 编辑器每次事务后 bump 一下版本号，让工具栏/菜单跟着选区刷新。 */
const tickObserver = $prose(
  () =>
    new Plugin({
      view: () => ({
        update: () => bumpEditor(),
      }),
    }),
)

/** vscode-editor-data 是 VS Code 系编辑器复制时写的 JSON，带被复制文件的语言。 */
function copiedFromMarkdownEditor(data: DataTransfer): boolean {
  const raw = data.getData('vscode-editor-data')
  if (!raw) return false
  try {
    const mode = (JSON.parse(raw) as { mode?: unknown }).mode
    return typeof mode === 'string' && /^(markdown|md)$/i.test(mode)
  } catch {
    return false
  }
}

/**
 * 文档头（主标题 / 说明块 / 题记）在**装载**时由文档面板用 `splitDoc` 摘出编辑器；
 * 粘贴整篇文档时也得走同一条路，否则正文里会多出一个 H1、露出裸的 `:::epigraph` 围栏。
 *
 * 同时也拦「从 markdown 编辑器复制来的整段」：默认剪贴板插件见到 vscode-editor-data 会把
 * 它原样包成一个代码块（v1 也专门绕开了这个），这里改按 markdown 解析回块。
 *
 * 插件插在默认剪贴板插件**之前**（`.use` 顺序）：两者都返回 false 时交给它。
 */
function pasteTransformPlugin(ref: { current: PasteTransform | undefined }) {
  return $prose(
    (ctx) =>
      new Plugin({
        props: {
          handlePaste: (view, event) => {
            const data = event.clipboardData
            const text = data?.getData('text/plain')
            if (!data || !text) return false
            const inCode = view.state.selection.$from.parent.type.spec.code
            let rest = text
            let handled = false
            const transform = ref.current
            if (transform) {
              const out = transform(text)
              if (out !== text) {
                rest = out
                handled = true
              }
            }
            if (!handled && !inCode && copiedFromMarkdownEditor(data)) handled = true
            if (!handled) return false
            if (rest.trim() === '') return true
            const slice = ctx.get(parserCtx)(rest)
            if (!slice || typeof slice === 'string') return false
            const dom = DOMSerializer.fromSchema(ctx.get(schemaCtx)).serializeFragment(slice.content)
            view.dispatch(
              view.state.tr.replaceSelection(
                DOMParser.fromSchema(ctx.get(schemaCtx)).parseSlice(dom),
              ),
            )
            return true
          },
        },
      }),
  )
}

/**
 * Markdown 编辑器（Milkdown）：存储格式就是 Markdown，所以文档模型与 `body_md`
 * 之间没有转换层——这是选它而不是 Tiptap 的原因。
 *
 * 预设只给语法，交互要自己装：`clipboard`（粘贴按 markdown 解析）、`history`（撤销）、
 * `cursor`/`indent`/`trailing`（间隙光标 / Tab 缩进 / 文末留白）、`tooltip`（选区工具栏）、
 * `slash`（`/` 菜单）、`block`（块拖拽手柄）。缺任何一个都会表现成「编辑器少块功能」。
 *
 * 用 `key={nodeId}` 在上层重建实例：Milkdown 只在挂载时吃一次 `defaultValue`。
 */
export default function MarkdownEditor({ initial, onChange, readOnly, transformPastedText, onAiWrite }: Props) {
  return (
    <MilkdownProvider>
      <Inner
        initial={initial}
        onChange={onChange}
        readOnly={readOnly}
        transformPastedText={transformPastedText}
        onAiWrite={onAiWrite}
      />
    </MilkdownProvider>
  )
}

function Inner({ initial, onChange, readOnly, transformPastedText, onAiWrite }: Props) {
  const onChangeRef = useRef(onChange)
  onChangeRef.current = onChange
  const transformRef = useRef(transformPastedText)
  transformRef.current = transformPastedText
  const readOnlyRef = useRef(!!readOnly)
  readOnlyRef.current = !!readOnly

  // 这三块 DOM 交给 Milkdown 的 provider 定位/显隐，React 用 portal 往里渲染。
  // 三个浮层 portal 到 body。给它们一个类名，好让 CSS 把它们钉成 fixed 挂点——
  // 否则它们是 body 里的静态块，会凭空撑出页面纵向滚动（见 editor.css `.editor-portal`）。
  const makePortalHost = () => {
    const el = document.createElement('div')
    el.className = 'editor-portal'
    return el
  }
  const [toolbarEl] = useState(makePortalHost)
  const [slashEl] = useState(makePortalHost)
  const [blockEl] = useState(makePortalHost)
  const blockProvider = useRef<BlockProvider | null>(null)

  const { loading, get } = useEditor(
    (root) =>
      Editor.make()
        .config((ctx) => {
          ctx.set(rootCtx, root)
          ctx.set(defaultValueCtx, initial)
          ctx.update(editorViewOptionsCtx, (prev) => ({
            ...prev,
            editable: () => !readOnlyRef.current,
          }))
          ctx.get(listenerCtx).markdownUpdated((_ctx, markdown) => {
            onChangeRef.current(markdown)
          })
          // 用 inject：$ctx 切片不会自动注册，set 会 "Context ... not found"。
          ctx.inject(toolbarSpec.key, {
            view: () => {
              const provider = new TooltipProvider({
                content: toolbarEl,
                root: document.body,
                floatingUIOptions: { strategy: 'fixed' },
              })
              return { update: provider.update, destroy: provider.destroy }
            },
          })
          ctx.inject(slashSpec.key, {
            view: () => {
              const provider = new SlashProvider({
                content: slashEl,
                root: document.body,
                floatingUIOptions: { strategy: 'fixed' },
                shouldShow: (view) => slashQuery(view) !== null,
              })
              provider.onShow = bumpEditor
              provider.onHide = bumpEditor
              return { update: provider.update, destroy: provider.destroy }
            },
          })
          ctx.inject(blockSpec.key, {
            view: () => {
              const provider = new BlockProvider({
                ctx,
                content: blockEl,
                root: document.body,
                getOffset: () => 6,
                floatingUIOptions: { strategy: 'fixed' },
              })
              blockProvider.current = provider
              return { update: provider.update, destroy: provider.destroy }
            },
          })
        })
        .use(commonmark)
        .use(gfm)
        .use(cursor)
        .use(indent)
        .use(trailing)
        .use(pasteTransformPlugin(transformRef))
        .use(clipboard)
        .use(history)
        .use(tickObserver)
        .use(toolbarPlugin)
        .use(slashPlugin)
        .use(block)
        .use(listener),
    [],
  )

  // 阅读态 ↔ 编辑态：改 EditorView 的 editable，不重建实例（重建会丢光标/历史）
  useEffect(() => {
    if (loading) return
    const editor = get()
    if (!editor) return
    editor.action((ctx) => ctx.get(editorViewCtx).setProps({ editable: () => !readOnly }))
  }, [loading, get, readOnly])

  useEditorTick()
  const editor = loading ? undefined : get()
  const empty = !!editor && !readOnly && isEmpty(editor)

  return (
    <>
      <div className="milkdown-wrap">
        <Milkdown />
        {empty && onAiWrite && (
          <div className="doc-empty-hint">
            <span>按 “/” 插入内容，或让</span>
            <button type="button" className="ai-inline-pill" onClick={onAiWrite}>
              ✦ AI 帮我写
            </button>
          </div>
        )}
      </div>
      {createPortal(<Toolbar />, toolbarEl)}
      {createPortal(<SlashMenu container={slashEl} />, slashEl)}
      {createPortal(<BlockHandle providerRef={blockProvider} />, blockEl)}
    </>
  )
}

function isEmpty(editor: Editor): boolean {
  try {
    return editor.action((ctx) => ctx.get(editorViewCtx).state.doc.textContent.trim() === '')
  } catch {
    return false
  }
}
