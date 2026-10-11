import type { Settings } from '../types'

/**
 * 设置页表单状态：来自后端 Settings 的扁平化镜像。
 * 文本输入一律字符串（空串 = 用默认值），数字同理；save 时统一转回 API 形状。
 * 与后端 API 的映射只发生在 toForm / SettingsView.save 两处，别在别处手搓。
 */
export interface FormState {
  endpoint: string
  model: string
  apiKey: string
  effort: string
  temperature: string
  maxTokens: string
  contextWindow: string
  exaApiKey: string
  proxyUrl: string
  embyUrl: string
  embyApiKey: string
  komgaUrl: string
  komgaApiKey: string
  gameatlasUrl: string
  gameatlasApiKey: string
  expireDays: number
  keepEventsDays: number
  maxConcurrentRuns: number
  adminUsername: string
  newNodeVisibility: 'public' | 'private'
  skillRoot: string
  scanInterval: string
}

export const EMPTY_FORM: FormState = {
  endpoint: '',
  model: '',
  apiKey: '',
  effort: 'medium',
  temperature: '',
  maxTokens: '',
  contextWindow: '',
  exaApiKey: '',
  proxyUrl: '',
  embyUrl: '',
  embyApiKey: '',
  komgaUrl: '',
  komgaApiKey: '',
  gameatlasUrl: '',
  gameatlasApiKey: '',
  expireDays: 7,
  keepEventsDays: 90,
  maxConcurrentRuns: 2,
  adminUsername: 'admin',
  newNodeVisibility: 'private',
  skillRoot: '',
  scanInterval: '360',
}

export function toForm(st: Settings): FormState {
  return {
    ...EMPTY_FORM,
    endpoint: st.llm.endpoint ?? '',
    model: st.llm.model ?? '',
    effort: st.llm.reasoningEffort || 'medium',
    temperature: st.llm.temperature != null ? String(st.llm.temperature) : '',
    maxTokens: st.llm.maxTokens != null ? String(st.llm.maxTokens) : '',
    contextWindow: st.llm.contextWindow ? String(st.llm.contextWindow) : '',
    proxyUrl: st.search.proxyUrl ?? '',
    embyUrl: st.library.embyUrl ?? '',
    komgaUrl: st.library.komgaUrl ?? '',
    gameatlasUrl: st.library.gameatlasUrl ?? '',
    expireDays: st.runs.expireDays,
    keepEventsDays: st.runs.keepEventsDays,
    maxConcurrentRuns: st.runs.maxConcurrentRuns,
    adminUsername: st.admin?.username ?? 'admin',
    newNodeVisibility: st.admin?.newNodeVisibility ?? 'private',
    skillRoot: st.skillRoot ?? '',
    scanInterval: st.library.scanIntervalMinutes != null ? String(st.library.scanIntervalMinutes) : '360',
  }
}

/** 精确到 key 类型的字段 setter：set('endpoint', 123) 编不过。 */
export type SetField = <K extends keyof FormState>(key: K, value: FormState[K]) => void
