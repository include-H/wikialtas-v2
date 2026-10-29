import { useEffect, useRef, useState } from 'react'
import {
  App as AntApp,
  Button,
  Input,
  List,
  Skeleton,
  Tag,
} from 'antd'
import { DownloadOutlined, UploadOutlined } from '@ant-design/icons'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { api } from '../api'

const SETTING_FIELDS = [
  ['llm.model', '模型'],
  ['llm.effort', '思考档位'],
  ['llm.context_window', '上下文窗口（token）'],
  ['llm.price_input_per_million', '输入价格 / 百万 token'],
  ['llm.price_output_per_million', '输出价格 / 百万 token'],
  ['library.scan_interval_minutes', '媒体库扫描周期（分钟，0=关）'],
  ['search.providers', '聚合搜索启用项（逗号分隔：exa,brave,deepseek）'],
  ['search.exa_base_url', 'Exa Base URL'],
  ['search.brave_base_url', 'Brave Base URL'],
  ['search.deepseek_base_url', 'DeepSeek Base URL'],
  ['research.max_search_calls', '研究检索调用上限'],
  ['research.max_fetch_calls', '研究网页抓取上限'],
  ['research.max_iterations', '研究最大轮次'],
] as const

const SECRET_FIELDS = [
  ['access.password', '访问密码（设置后全站需登录）'],
  ['exa.api_key', 'Exa API Key'],
  ['brave.api_key', 'Brave Search API Key'],
  ['deepseek.api_key', 'DeepSeek API Key（搜索/抓取也可用）'],
  ['llm.base_url', 'LLM Base URL'],
  ['llm.api_key', 'LLM API Key'],
  ['gameatlas.base_url', 'GameAtlas URL'],
  ['gameatlas.password', 'GameAtlas 管理员密码'],
  ['emby.base_url', 'Emby URL'],
  ['emby.token', 'Emby Token'],
  ['komga.base_url', 'Komga URL'],
  ['komga.api_key', 'Komga API Key'],
] as const

export default function SettingsPanel() {
  const qc = useQueryClient()
  const { message } = AntApp.useApp()
  const [cfg, setCfg] = useState<Record<string, string>>({})
  const [secrets, setSecrets] = useState<Record<string, string>>({})
  const fileRef = useRef<HTMLInputElement | null>(null)

  const settings = useQuery({ queryKey: ['settings'], queryFn: api.settings })
  const usage = useQuery({ queryKey: ['usage'], queryFn: api.usage })

  useEffect(() => {
    if (!settings.data) return
    const next: Record<string, string> = {}
    for (const [key] of SETTING_FIELDS) {
      const v = settings.data.settings[key]
      next[key] = Array.isArray(v) ? v.join(',') : v == null ? '' : String(v)
    }
    setCfg(next)
  }, [settings.data])

  const save = useMutation({
    mutationFn: async () => {
      const app: Record<string, unknown> = {}
      for (const [key] of SETTING_FIELDS) {
        const raw = cfg[key]?.trim() ?? ''
        if (!raw) continue
        if (key === 'search.providers') {
          app[key] = raw.split(',').map((x) => x.trim()).filter(Boolean)
        } else if (
          key.includes('window') ||
          key.includes('price') ||
          key.includes('interval') ||
          key.includes('research.max_')
        ) {
          const n = Number(raw)
          if (!Number.isFinite(n) || n < 0) throw new Error(`${key} 必须是非负数`)
          app[key] = n
        } else {
          app[key] = raw
        }
      }
      if (Object.keys(app).length) await api.putSettings(app)
      const hidden = Object.fromEntries(
        Object.entries(secrets).filter(([, v]) => v.trim().length > 0),
      )
      if (Object.keys(hidden).length) await api.putSecrets(hidden)
    },
    onSuccess: () => {
      setSecrets({})
      qc.invalidateQueries({ queryKey: ['settings'] })
      qc.invalidateQueries({ queryKey: ['usage'] })
      message.success('设置已保存；密钥值不会回显')
    },
    onError: (e) => message.error(e.message),
  })

  const exportProject = useMutation({
    mutationFn: api.exportProject,
    onSuccess: (bundle) => {
      const blob = new Blob([JSON.stringify(bundle, null, 2)], { type: 'application/json' })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `wikiatlas-tree-${new Date().toISOString().slice(0, 10)}.json`
      a.click()
      URL.revokeObjectURL(url)
    },
    onError: (e) => message.error(`导出失败：${e.message}`),
  })

  const importProject = useMutation({
    mutationFn: async (file: File) => {
      const raw = await file.text()
      return api.importProject(JSON.parse(raw) as Record<string, unknown>)
    },
    onSuccess: (out) => {
      qc.invalidateQueries({ queryKey: ['tree'] })
      qc.invalidateQueries({ queryKey: ['overview'] })
      message.success(`已导入 ${out.nodes} 个节点`)
    },
    onError: (e) => message.error(`导入失败：${e.message}`),
  })

  if (settings.isLoading) {
    return <div className="settings-page"><Skeleton active paragraph={{ rows: 12 }} /></div>
  }

  return (
    <div className="settings-page">
      <div className="settings-inner">
        <div className="settings-head">
          <div>
            <h1>设置与用量</h1>
            <p>非密钥配置在这里；密钥只显示“已配置”，永远不回显原值。</p>
          </div>
          <Button type="primary" loading={save.isPending} onClick={() => save.mutate()}>
            保存
          </Button>
        </div>

        <section className="settings-section">
          <h2>模型与扫描</h2>
          <div className="settings-grid">
            {SETTING_FIELDS.map(([key, label]) => (
              <label key={key} className="settings-field">
                <span>{label}</span>
                <Input
                  value={cfg[key] ?? ''}
                  onChange={(e) => setCfg((c) => ({ ...c, [key]: e.target.value }))}
                  placeholder={key === 'llm.model' ? '例如 Qwen-Uncensored' : ''}
                />
              </label>
            ))}
          </div>
        </section>

        <section className="settings-section">
          <h2>连接与密钥</h2>
          <div className="settings-grid">
            {SECRET_FIELDS.map(([key, label]) => (
              <label key={key} className="settings-field">
                <span>
                  {label}
                  {settings.data?.secrets_configured[key] ? (
                    <Tag color="green" style={{ marginLeft: 6 }}>已配置</Tag>
                  ) : null}
                </span>
                <Input.Password
                  value={secrets[key] ?? ''}
                  onChange={(e) => setSecrets((s) => ({ ...s, [key]: e.target.value }))}
                  placeholder={settings.data?.secrets_configured[key] ? '留空 = 不修改' : '未配置'}
                  autoComplete="new-password"
                />
              </label>
            ))}
          </div>
        </section>

        <section className="settings-section">
          <h2>用量</h2>
          {usage.isLoading ? (
            <Skeleton active />
          ) : usage.data ? (
            <>
              <div className="usage-cards">
                <UsageCard label="工单" value={String(usage.data.runs)} />
                <UsageCard label="总 token" value={fmt(usage.data.total_tokens)} />
                <UsageCard label="输入" value={fmt(usage.data.input_tokens)} />
                <UsageCard label="输出" value={fmt(usage.data.output_tokens)} />
                <UsageCard
                  label="估算成本"
                  value={usage.data.estimated_cost == null ? '未配置单价' : `$${usage.data.estimated_cost.toFixed(2)}`}
                />
              </div>
              <List
                size="small"
                dataSource={usage.data.models}
                locale={{ emptyText: '还没有模型用量' }}
                renderItem={(m) => (
                  <List.Item>
                    <span>{m.model}</span>
                    <span className="usage-line">
                      {m.runs} 工单 · {fmt(m.total_tokens)} token
                      {m.estimated_cost == null ? '' : ` · $${m.estimated_cost.toFixed(2)}`}
                    </span>
                  </List.Item>
                )}
              />
            </>
          ) : null}
        </section>

        <section className="settings-section">
          <h2>项目导出 / 导入</h2>
          <p className="settings-note">
            导出节点、正文、快照、关系与资料引用，不包含密钥、运行日志和媒体库密钥。
            导入只允许写进空树。
          </p>
          <div className="settings-actions">
            <Button
              icon={<DownloadOutlined />}
              loading={exportProject.isPending}
              onClick={() => exportProject.mutate()}
            >
              导出 JSON
            </Button>
            <Button
              icon={<UploadOutlined />}
              loading={importProject.isPending}
              onClick={() => fileRef.current?.click()}
            >
              导入 JSON
            </Button>
            <input
              ref={fileRef}
              type="file"
              accept="application/json,.json"
              hidden
              onChange={(e) => {
                const file = e.target.files?.[0]
                if (file) importProject.mutate(file)
                e.target.value = ''
              }}
            />
          </div>
        </section>
      </div>
    </div>
  )
}

function UsageCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="usage-card">
      <span>{value}</span>
      <small>{label}</small>
    </div>
  )
}

function fmt(n: number): string {
  return new Intl.NumberFormat('zh-CN').format(n)
}
