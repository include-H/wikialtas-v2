import { Card, Input, Select } from '@douyinfe/semi-ui'
import { ConfiguredTag, Text, type SectionProps } from './common'

interface Props extends SectionProps {
  adminPassword: string
  onAdminPassword: (v: string) => void
}

/** 访问与隐私：管理态入口 + 访客可见性。出厂密码告警已提升到页面级横幅。 */
export function AccountCard({ form, set, settings, adminPassword, onAdminPassword }: Props) {
  return (
    <Card title="访问与隐私" className="settings-card">
      <Text type="tertiary" size="small">
        访问密码是管理态的唯一入口：**未登录只能读公开文档，私有内容一律看不到**
        （公开要求自身与所有祖先都是公开，资料继承所属节点）。
      </Text>
      <div className="settings-field-inline settings-account-fields">
        <div className="settings-field">
          <span className="settings-label">管理员标识（仅显示用）</span>
          <Input value={form.adminUsername} onChange={(v) => set('adminUsername', v)} />
          <Text type="tertiary" size="small">
            Altas 会知道它在给谁做事（被问「我是谁」时用得上）。它只是显示名，
            不参与登录、不代表任何权限。
          </Text>
        </div>
        <div className="settings-field">
          <span className="settings-label">
            访问密码 <ConfiguredTag configured={settings.admin.passwordConfigured} />
          </span>
          <Input
            mode="password"
            value={adminPassword}
            onChange={onAdminPassword}
            placeholder={
              settings.admin.passwordConfigured ? '已设置，留空不修改' : '设置一个密码以启用登录'
            }
          />
        </div>
        <div className="settings-field">
          <span className="settings-label">新节点默认可见性</span>
          <Select<string>
            value={form.newNodeVisibility}
            onChange={(v) => set('newNodeVisibility', String(v) as 'public' | 'private')}
            optionList={[
              { value: 'private', label: '私有（推荐）' },
              { value: 'public', label: '公开' },
            ]}
            style={{ width: '100%' }}
          />
        </div>
      </div>
      <Text type="tertiary" size="small">
        单作 / 系列 / 宇宙都可以单独设为公开或私有；把父节点设为私有时，其下所有内容对访客一并隐藏。
      </Text>
    </Card>
  )
}
