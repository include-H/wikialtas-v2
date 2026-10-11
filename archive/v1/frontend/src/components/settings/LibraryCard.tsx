import { useMemo } from 'react'
import { Banner, Button, Card, Input, InputNumber, Table } from '@douyinfe/semi-ui'
import type { ColumnProps } from '@douyinfe/semi-ui/lib/es/table/interface'
import type { EmbyLibraryView } from '../../types'
import { Field, RoleTag, ConfiguredTag, Text, type EmbyRole, type SectionProps } from './common'

interface Props extends SectionProps {
  embyViews: EmbyLibraryView[] | null
  embyRoles: Record<string, EmbyRole>
  onRefreshEmby: () => void
  onEditRoles: () => void
}

function MutedType({ value }: { value?: string }) {
  return <Text type="tertiary" size="small">{value || '—'}</Text>
}

export function LibraryCard({ form, set, settings, embyViews, embyRoles, onRefreshEmby, onEditRoles }: Props) {
  const views = embyViews ?? []

  const readColumns = useMemo<ColumnProps<EmbyLibraryView>[]>(
    () => [
      { title: '库', dataIndex: 'name', render: (t: string) => <span title={t}>{t}</span> },
      { title: '类型', dataIndex: 'collectionType', width: 110, render: (t?: string) => <MutedType value={t} /> },
      {
        title: '角色',
        width: 140,
        render: (_: unknown, r: EmbyLibraryView) => <RoleTag role={embyRoles[r.id] ?? ''} />,
      },
    ],
    [embyRoles],
  )

  const canEdit = views.length > 0

  return (
    <Card title="媒体库" className="settings-card">
      <Banner
        type="info"
        closeIcon={null}
        description="Emby（正片建档 / 混杂关联）、GameAtlas（建档 / 关联 / 反哺）、Komga（漫画小说建档 / 关联 / 反哺简介）均已接入。"
      />
      <div className="settings-field-inline">
        <div className="settings-field">
          <span className="settings-label">Emby URL</span>
          <Input
            value={form.embyUrl}
            onChange={(v) => set('embyUrl', v)}
            placeholder="http://192.168.1.4:8081/emby"
          />
        </div>
        <div className="settings-field">
          <span className="settings-label">
            Emby API Key <ConfiguredTag configured={settings.library.embyApiKeyConfigured} />
          </span>
          <Input
            mode="password"
            value={form.embyApiKey}
            onChange={(v) => set('embyApiKey', v)}
            placeholder={
              settings.library.embyApiKeyConfigured
                ? '已配置，留空不修改'
                : '在 Emby 后台 高级→安全 里生成'
            }
          />
        </div>
      </div>

      <div className="settings-field">
        <div className="roles-head">
          <span className="settings-label">
            Emby 媒体库角色{embyViews !== null ? `（${views.length}）` : ''}
          </span>
          <div className="roles-head-actions">
            <Button size="small" theme="borderless" onClick={onRefreshEmby}>
              刷新库清单
            </Button>
            <Button size="small" theme="solid" disabled={!canEdit} onClick={onEditRoles}>
              编辑角色
            </Button>
          </div>
        </div>
        <Text type="tertiary" size="small">
          正片（Work）：集合页给「建议建档」；混杂内容（Mixed）：节点页给「关联候选」（第九艺术/解析这类）。
        </Text>
        {embyViews === null ? (
          <Text type="tertiary" size="small">
            保存 Emby 地址与 Key 后点「刷新库清单」。
          </Text>
        ) : views.length === 0 ? (
          <Text type="tertiary" size="small">
            没拉到媒体库（检查地址与 Key）。
            <Button size="small" theme="borderless" onClick={onRefreshEmby}>
              重试
            </Button>
          </Text>
        ) : (
          <Table
            className="settings-roles-table"
            rowKey="id"
            columns={readColumns}
            dataSource={views}
            pagination={false}
            size="small"
            scroll={{ y: 320 }}
          />
        )}
      </div>

      <div className="settings-field-inline">
        <div className="settings-field">
          <span className="settings-label">Komga URL</span>
          <Input
            value={form.komgaUrl}
            onChange={(v) => set('komgaUrl', v)}
            placeholder="http://192.168.1.4:8082（带不带 /api/v1 都行）"
          />
        </div>
        <div className="settings-field">
          <span className="settings-label">
            Komga API Key <ConfiguredTag configured={settings.library.komgaApiKeyConfigured} />
          </span>
          <Input
            mode="password"
            value={form.komgaApiKey}
            onChange={(v) => set('komgaApiKey', v)}
            placeholder={
              settings.library.komgaApiKeyConfigured
                ? '已配置，留空不修改'
                : '在 Komga 用户设置里生成 API Key'
            }
          />
        </div>
      </div>

      <div className="settings-field-inline">
        <div className="settings-field">
          <span className="settings-label">GameAtlas URL</span>
          <Input
            value={form.gameatlasUrl}
            onChange={(v) => set('gameatlasUrl', v)}
            placeholder="http://192.168.1.4:3000（你的 GameAtlas 地址）"
          />
        </div>
        <div className="settings-field">
          <span className="settings-label">
            GameAtlas 管理员密码 <ConfiguredTag configured={settings.library.gameatlasApiKeyConfigured} />
          </span>
          <Input
            mode="password"
            value={form.gameatlasApiKey}
            onChange={(v) => set('gameatlasApiKey', v)}
            placeholder={
              settings.library.gameatlasApiKeyConfigured
                ? '已配置，留空不修改'
                : '它没有 API Key——填管理员密码'
            }
          />
        </div>
      </div>

      <Field
        label="媒体库后台扫描间隔（分钟）"
        hint="每 N 分钟自动扫一次媒体库清单（建议池读它）；0 = 关闭自动扫描、只手动扫。"
      >
        <InputNumber
          value={form.scanInterval === '' ? undefined : Number(form.scanInterval)}
          onChange={(v) => set('scanInterval', v == null ? '' : String(v))}
          min={0}
          max={10080}
          step={60}
          style={{ width: 220 }}
        />
      </Field>
    </Card>
  )
}
