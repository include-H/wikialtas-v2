import { useCallback, useEffect, useState } from 'react'
import { Banner, Button, Spin, Tabs, Toast, Typography } from '@douyinfe/semi-ui'
import type { EmbyLibraryView, RuntimeInfo, Settings } from '../types'
import { getEmbyViews, getRuntime, getSettings, putSettings, testLLM } from '../lib/api'
import { useAppStore } from '../lib/store'
import { EMPTY_FORM, toForm, type FormState, type SetField } from '../lib/settingsForm'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { LlmCard, type TestResult } from '../components/settings/LlmCard'
import { SearchCard } from '../components/settings/SearchCard'
import { RunsCard, SkillCard } from '../components/settings/WorkCards'
import { LibraryCard } from '../components/settings/LibraryCard'
import { RoleEditorModal } from '../components/settings/RoleEditorModal'
import type { EmbyRole } from '../components/settings/common'
import { AccountCard } from '../components/settings/AccountCard'
import { StatusCard } from '../components/settings/StatusCard'

const { Title, Text } = Typography

const TAB_KEYS = ['service', 'work', 'library', 'account', 'status'] as const
type TabKey = (typeof TAB_KEYS)[number]

export default function SettingsView() {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [runtime, setRuntime] = useState<RuntimeInfo | null>(null)
  const [form, setForm] = useState<FormState>(EMPTY_FORM)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [testing, setTesting] = useState(false)
  const [testResult, setTestResult] = useState<TestResult | null>(null)
  const { me } = useAppStore()
  const nav = useNavigate()
  const [searchParams, setSearchParams] = useSearchParams()
  const [adminPassword, setAdminPassword] = useState('')
  const [embyViews, setEmbyViews] = useState<EmbyLibraryView[] | null>(null)
  const [embyRoles, setEmbyRoles] = useState<Record<string, EmbyRole>>({})
  const [roleEditorOpen, setRoleEditorOpen] = useState(false)

  const tab: TabKey = TAB_KEYS.includes(searchParams.get('tab') as TabKey)
    ? (searchParams.get('tab') as TabKey)
    : 'service'

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const [st, rt] = await Promise.all([getSettings(), getRuntime().catch(() => null)])
      setSettings(st)
      setForm(toForm(st))
      setRuntime(rt)
    } catch (e) {
      Toast.error(e instanceof Error ? e.message : '加载设置失败')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    if (me.authed) void load()
  }, [load, me.authed])

  const loadEmbyViews = useCallback(async () => {
    try {
      const res = await getEmbyViews()
      setEmbyViews(res.views ?? [])
    } catch {
      setEmbyViews([])
    }
  }, [])

  // Emby 地址配好后自动拉一次库清单；拉到后把已存的角色映射到界面
  useEffect(() => {
    if (!me.authed || !settings?.library.embyUrl) return
    void loadEmbyViews()
  }, [me.authed, settings?.library.embyUrl, loadEmbyViews])

  useEffect(() => {
    if (!embyViews || !settings) return
    const roles = settings.library.embyLibraryRoles ?? []
    const next: Record<string, EmbyRole> = {}
    for (const v of embyViews) {
      const hit = roles.find((r) => (r.id && r.id === v.id) || r.name === v.name)
      next[v.id] = hit ? hit.role : ''
    }
    setEmbyRoles(next)
  }, [embyViews, settings])

  const set: SetField = (key, value) => setForm((prev) => ({ ...prev, [key]: value }))

  const save = async (opts?: { rolesOverride?: Record<string, EmbyRole> }): Promise<boolean> => {
    const roles = opts?.rolesOverride ?? embyRoles
    setSaving(true)
    try {
      const next = await putSettings({
        llm: {
          endpoint: form.endpoint,
          model: form.model,
          reasoningEffort: form.effort,
          apiKey: form.apiKey || undefined,
          temperature: form.temperature ? Number(form.temperature) : undefined,
          maxTokens: form.maxTokens ? Number(form.maxTokens) : undefined,
          contextWindow: form.contextWindow ? Number(form.contextWindow) : undefined,
        },
        search: { exaApiKey: form.exaApiKey || undefined, proxyUrl: form.proxyUrl },
        library: {
          embyUrl: form.embyUrl || undefined,
          embyApiKey: form.embyApiKey || undefined,
          komgaUrl: form.komgaUrl || undefined,
          komgaApiKey: form.komgaApiKey || undefined,
          gameatlasUrl: form.gameatlasUrl || undefined,
          gameatlasApiKey: form.gameatlasApiKey || undefined,
          // 库清单没拉到就不要提交这个字段（非 nil = 整体替换，会把已有角色清掉）
          ...(embyViews !== null
            ? {
                embyLibraryRoles: embyViews
                  .filter((v) => roles[v.id])
                  .map((v) => ({ id: v.id, name: v.name, role: roles[v.id] as 'work' | 'mixed' })),
              }
            : {}),
          scanIntervalMinutes: form.scanInterval === '' ? undefined : Number(form.scanInterval),
        },
        runs: {
          expireDays: form.expireDays,
          keepEventsDays: form.keepEventsDays,
          maxConcurrentRuns: form.maxConcurrentRuns,
        },
        admin: {
          username: form.adminUsername,
          newPassword: adminPassword || undefined,
          newNodeVisibility: form.newNodeVisibility,
        },
        skillRoot: form.skillRoot,
      })
      setSettings(next)
      if (opts?.rolesOverride) setEmbyRoles(opts.rolesOverride)
      setForm({ ...toForm(next), apiKey: '', exaApiKey: '', embyApiKey: '', komgaApiKey: '', gameatlasApiKey: '' })
      setAdminPassword('')
      Toast.success('已保存（下一个工单即生效，无需重启）')
      setRuntime(await getRuntime().catch(() => null))
      void loadEmbyViews()
      return true
    } catch (e) {
      Toast.error(e instanceof Error ? e.message : '保存失败')
      return false
    } finally {
      setSaving(false)
    }
  }

  /** 角色弹窗的提交：草稿 → 全局保存；成功才关窗（失败留在弹窗里可重试） */
  const commitRoles = async (roles: Record<string, EmbyRole>) => {
    const ok = await save({ rolesOverride: roles })
    if (ok) setRoleEditorOpen(false)
    return ok
  }

  const runTest = async () => {
    setTesting(true)
    setTestResult(null)
    try {
      const res = await testLLM()
      setTestResult(
        res.ok
          ? { ok: true, text: `${res.model} 正常（${res.latencyMs}ms）${res.reply ? ` · 回复：${res.reply}` : ''}` }
          : { ok: false, text: `连接失败：${res.error ?? '未知错误'}` },
      )
    } catch (e) {
      setTestResult({ ok: false, text: e instanceof Error ? e.message : '请求失败' })
    } finally {
      setTesting(false)
    }
  }

  const switchTab = (key: string) => {
    setSearchParams({ tab: key }, { replace: true })
  }

  if (loading && !settings) {
    return <Spin style={{ display: 'block', margin: '64px auto' }} />
  }

  // 访客不进入设置页（接口本身也会 401，这里给一个明确的入口）
  if (!me.authed) {
    return (
      <div className="settings-view">
        <div className="settings-inner">
          <Banner type="info" description="设置需要登录后才能查看与修改。" />
          <Button style={{ marginTop: 12 }} theme="solid" type="primary" onClick={() => nav('/login')}>
            去登录
          </Button>
        </div>
      </div>
    )
  }

  const sectionProps = { form, set, settings: settings as Settings }

  return (
    <div className="settings-view">
      {/* 限宽放**内层**：滚动容器（.settings-view）通栏，滚动条落在窗口右边。
          以前 max-width 加在滚动容器自身上，宽屏下容器 900px 居中，
          滚动条悬在屏幕中间、右侧一大片死白（用户截图）。 */}
      <div className="settings-inner">
        <div className="settings-head">
          <Title heading={4} style={{ margin: 0 }}>
            设置
          </Title>
          <div className="settings-head-actions">
            <Button theme="solid" type="primary" loading={saving} onClick={() => void save()}>
              保存
            </Button>
          </div>
        </div>
        <Text type="tertiary" size="small">
          配置存在本地 SQLite；保存后立刻对下一个工单生效。
        </Text>

        {settings?.admin.usingDefaultPassword && (
          <Banner
            type="warning"
            closeIcon={null}
            className="settings-alert"
            description="当前仍是出厂密码 1234，建议现在就改掉（账户 → 访问密码）：改完所有旧会话会立即失效。"
          />
        )}

        <Tabs activeKey={tab} onChange={switchTab} className="settings-tabs">
          <Tabs.TabPane itemKey="service" tab="服务连接">
            <div className="settings-grid">
              <LlmCard {...sectionProps} testing={testing} testResult={testResult} onTest={() => void runTest()} />
              <SearchCard {...sectionProps} />
            </div>
          </Tabs.TabPane>
          <Tabs.TabPane itemKey="work" tab="工作参数">
            <div className="settings-grid">
              <RunsCard {...sectionProps} />
              <SkillCard {...sectionProps} runtime={runtime} />
            </div>
          </Tabs.TabPane>
          <Tabs.TabPane itemKey="library" tab="媒体库">
            <div className="settings-grid">
              <LibraryCard
                {...sectionProps}
                embyViews={embyViews}
                embyRoles={embyRoles}
                onRefreshEmby={() => void loadEmbyViews()}
                onEditRoles={() => setRoleEditorOpen(true)}
              />
            </div>
          </Tabs.TabPane>
          <Tabs.TabPane itemKey="account" tab="账户">
            <div className="settings-grid">
              <AccountCard {...sectionProps} adminPassword={adminPassword} onAdminPassword={setAdminPassword} />
            </div>
          </Tabs.TabPane>
          <Tabs.TabPane itemKey="status" tab="状态">
            <div className="settings-grid">
              <StatusCard runtime={runtime} />
            </div>
          </Tabs.TabPane>
        </Tabs>
      </div>
      <RoleEditorModal
        open={roleEditorOpen}
        views={embyViews ?? []}
        initial={embyRoles}
        committing={saving}
        onCancel={() => setRoleEditorOpen(false)}
        onCommit={commitRoles}
      />
    </div>
  )
}
