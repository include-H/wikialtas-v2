import type { ReactNode } from 'react'
import { Tag, Typography } from '@douyinfe/semi-ui'
import type { RuntimeInfo, Settings } from '../../types'
import type { FormState, SetField } from '../../lib/settingsForm'

export const { Text } = Typography

/** label 旁的「已配置 / 未配置」小徽标（密码类字段的当前状态提示） */
export function ConfiguredTag({ configured }: { configured?: boolean }) {
  return configured ? (
    <Tag size="small" color="green">已配置</Tag>
  ) : (
    <Tag size="small" color="grey">未配置</Tag>
  )
}

/**
 * 标准设置项：label → 控件 → 说明。
 * 说明文案一律走 hint（type=tertiary small），保证全页层级一致。
 */
export function Field({
  label,
  hint,
  children,
}: {
  label: ReactNode
  hint?: ReactNode
  children: ReactNode
}) {
  return (
    <div className="settings-field">
      <span className="settings-label">{label}</span>
      {children}
      {hint ? <Text type="tertiary" size="small">{hint}</Text> : null}
    </div>
  )
}

/** Emby 库角色：work = 正片（建档建议）；mixed = 混杂内容（关联候选）；'' = 不参与 */
export type EmbyRole = 'work' | 'mixed' | ''

export function RoleTag({ role }: { role: EmbyRole }) {
  if (role === 'work') return <Tag size="small" color="blue">正片</Tag>
  if (role === 'mixed') return <Tag size="small" color="orange">混杂内容</Tag>
  return <Tag size="small" color="grey">不参与</Tag>
}

/** 各分组卡片的公共 props */
export interface SectionProps {
  form: FormState
  set: SetField
  settings: Settings
}

export interface RuntimeSectionProps extends SectionProps {
  runtime: RuntimeInfo | null
}
