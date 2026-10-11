import { Card, Input, InputNumber } from '@douyinfe/semi-ui'
import { Field, Text, type RuntimeSectionProps, type SectionProps } from './common'

/** 工单与并发：影响「一次能跑多少、留下多少」的运行参数 */
export function RunsCard({ form, set }: SectionProps) {
  return (
    <Card title="工单与并发" className="settings-card">
      <div className="settings-field-inline">
        <div className="settings-field">
          <span className="settings-label">并发上限（部/次）</span>
          <InputNumber
            value={form.maxConcurrentRuns}
            onChange={(v) => set('maxConcurrentRuns', Number(v ?? 2))}
            min={1}
            max={8}
            style={{ width: '100%' }}
          />
        </div>
        <div className="settings-field">
          <span className="settings-label">中断工单保留（天）</span>
          <InputNumber
            value={form.expireDays}
            onChange={(v) => set('expireDays', Number(v ?? 7))}
            min={1}
            max={90}
            style={{ width: '100%' }}
          />
        </div>
        <div className="settings-field">
          <span className="settings-label">事件保留（天）</span>
          <InputNumber
            value={form.keepEventsDays}
            onChange={(v) => set('keepEventsDays', Number(v ?? 90))}
            min={7}
            max={365}
            style={{ width: '100%' }}
          />
        </div>
      </div>
      <Text type="tertiary" size="small">
        并发数改完立即生效（worker 池固定上限、闸门实时读设置）。批次建档也走这个上限。
      </Text>
    </Card>
  )
}

/** 写作 Skill：create_wiki 按 skills/wiki-writing 加载骨架与媒体规则 */
export function SkillCard({ form, set, runtime }: RuntimeSectionProps) {
  return (
    <Card title="写作 Skill" className="settings-card">
      <Field
        label="Skill 目录"
        hint={
          <>
            当前生效：<code>{runtime?.skillRoot || '未解析到'}</code>
            <br />
            create_wiki 加载 SKILL.md → core.md → 一个 media-*.md；运行时默认不注入 examples/。
          </>
        }
      >
        <Input
          value={form.skillRoot}
          onChange={(v) => set('skillRoot', v)}
          placeholder="/root/WikiAltas/skills/wiki-writing"
        />
      </Field>
    </Card>
  )
}
