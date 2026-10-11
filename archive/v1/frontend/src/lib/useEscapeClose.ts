import { useEffect } from 'react'

/**
 * ESC 关闭(open 时生效)。
 *
 * 为什么不用 Semi Modal 自带的 closeOnEsc:它的文档级监听走**冒泡**阶段,
 * 而焦点在弹窗外(例如刚打开、焦点还停在触发按钮上)时,自然 ESC 事件在
 * React 合成层就被截了,到不了 document——弹窗关不掉(实测复现)。
 * 这里用 **capture 阶段**的 document 监听,早于一切截断,行为确定。
 */
export function useEscapeClose(open: boolean, onClose: () => void) {
  useEffect(() => {
    if (!open) return
    const h = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', h, true)
    return () => document.removeEventListener('keydown', h, true)
  }, [open, onClose])
}
