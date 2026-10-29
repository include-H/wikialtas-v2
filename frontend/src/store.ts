import { create } from 'zustand'

// 客户端状态只放「不是服务端数据」的东西（`docs/frontend.md` §1）：
// 面板开合、当前选中。正文、树、版本一律归 TanStack Query 管。
interface UiState {
  /** 主区看什么：文档编辑（v1 主页）还是媒体库池子。v1 的全局导航两项。 */
  view: 'doc' | 'pool' | 'audit' | 'search' | 'settings'
  setView: (view: 'doc' | 'pool' | 'audit' | 'search' | 'settings') => void

  selectedId: string | null
  select: (id: string | null) => void

  /** 阅读/编辑态（v1 的编辑器顶栏眼睛图标；顶栏与世界共用）。 */
  docMode: 'edit' | 'proofread' | 'read'
  setDocMode: (mode: 'edit' | 'proofread' | 'read') => void

  /**
   * 工具栏「问 Atlas / 翻译这段 / 解释这段」把选段 + 说法投递给 AI 面板。
   * 模型这轮还没接（尖刺 B），先落到 store 让右栏能显示"收到哪段、要它做什么"。
   */
  selectionAsk: { text: string; prompt: string; autoSend: boolean } | null
  askSelection: (payload: { text: string; prompt: string; autoSend: boolean }) => void
  clearSelectionAsk: () => void

  /** 开着 AI 面板（窄屏是抽屉）。 */
  aiOpen: boolean
  setAiOpen: (open: boolean) => void

  /** 「让 Atlas 写」把一句话投进 Atlas 输入框（`design.md §6.3`）。 */
  aiPrefill: string
  prefillAi: (text: string) => void
  clearAiPrefill: () => void

  /**
   * 实时写入：当前正在写哪个节点、写到哪了（`design.md §6.3` 的只读实时视图）。
   * **服务端草稿才是真相**，这里只是加速通道的展示（D46）。
   */
  liveWrite: { nodeId: string; bodyMd: string; phase: string } | null
  setLiveWrite: (v: { nodeId: string; bodyMd: string; phase: string } | null) => void
}

export const useUi = create<UiState>((set) => ({
  view: 'doc',
  setView: (view) => set({ view }),

  selectedId: null,
  select: (id) => set({ selectedId: id }),

  docMode: 'edit',
  setDocMode: (docMode) => set({ docMode }),

  selectionAsk: null,
  askSelection: (payload) => set({ selectionAsk: payload, aiOpen: true }),
  clearSelectionAsk: () => set({ selectionAsk: null }),

  aiOpen: false,
  setAiOpen: (aiOpen) => set({ aiOpen }),

  aiPrefill: '',
  prefillAi: (text) => set({ aiPrefill: text, aiOpen: true }),
  clearAiPrefill: () => set({ aiPrefill: '' }),

  liveWrite: null,
  setLiveWrite: (liveWrite) => set({ liveWrite }),
}))
