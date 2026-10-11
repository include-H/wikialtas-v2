import { create } from 'zustand'

// 客户端状态只放「不是服务端数据」的东西（`docs/frontend.md` §1）：
// 面板开合、当前选中。正文、树、版本一律归 TanStack Query 管。
interface UiState {
  selectedId: string | null
  select: (id: string | null) => void

  /** 阅读/编辑态（v1 的编辑器顶栏眼睛图标；顶栏与世界共用）。 */
  docMode: 'edit' | 'read'
  setDocMode: (mode: 'edit' | 'read') => void

  /**
   * 工具栏「问 Altas / 翻译这段 / 解释这段」把选段 + 说法投递给 AI 面板。
   * 模型这轮还没接（尖刺 B），先落到 store 让右栏能显示"收到哪段、要它做什么"。
   */
  selectionAsk: { text: string; prompt: string; autoSend: boolean } | null
  askSelection: (payload: { text: string; prompt: string; autoSend: boolean }) => void
  clearSelectionAsk: () => void

  /** 开着 AI 面板（窄屏是抽屉）。 */
  aiOpen: boolean
  setAiOpen: (open: boolean) => void
}

export const useUi = create<UiState>((set) => ({
  selectedId: null,
  select: (id) => set({ selectedId: id }),

  docMode: 'edit',
  setDocMode: (docMode) => set({ docMode }),

  selectionAsk: null,
  askSelection: (payload) => set({ selectionAsk: payload, aiOpen: true }),
  clearSelectionAsk: () => set({ selectionAsk: null }),

  aiOpen: false,
  setAiOpen: (aiOpen) => set({ aiOpen }),
}))
