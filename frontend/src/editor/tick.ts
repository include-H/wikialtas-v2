import { useSyncExternalStore } from 'react'

/**
 * 编辑器状态版本号。工具栏要不要高亮「加粗」、斜杠菜单该不该开着，都得跟着
 * 选区/文档变——Milkdown 的 React 绑定只给 `useInstance`，没有选区订阅，
 * 所以由 `MarkdownEditor` 里一个 observer 插件在每次事务后 bump 一下，
 * UI 组件用 `useEditorTick` 订阅它重渲染。
 *
 * **必须异步、必须合并**：observer 的 `update` 是在 ProseMirror 的
 * `updatePluginViews` 里同步跑的（见调用栈），同步通知 `useSyncExternalStore`
 * 会让 React 19 立刻重渲染 → 又触发一次事务 → `Maximum update depth exceeded`。
 * 所以 bump 只登记一帧，真正发通知放到 `requestAnimationFrame`。
 *
 * 全应用同时只有一个正文编辑器（三面板里文档区仅一处），所以这份订阅是
 * 模块级的单例，不做多实例隔离。
 */
const listeners = new Set<() => void>()
let version = 0
let frame = 0

function flush(): void {
  frame = 0
  version += 1
  for (const listener of listeners) listener()
}

/** 登记一次「编辑器状态变了」；一帧内的多次调用合并成一次通知。 */
export function bumpEditor(): void {
  if (frame) return
  frame = requestAnimationFrame(flush)
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function snapshot(): number {
  return version
}

/** 订阅编辑器状态变更（返回版本号）；在组件里直接调用即可。 */
export function useEditorTick(): number {
  return useSyncExternalStore(subscribe, snapshot, snapshot)
}
