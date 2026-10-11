import { Button, Card, Input, InputNumber, Select } from '@douyinfe/semi-ui'
import { ConfiguredTag, Field, Text, type SectionProps } from './common'

export interface TestResult {
  ok: boolean
  text: string
}

interface Props extends SectionProps {
  testing: boolean
  testResult: TestResult | null
  onTest: () => void
}

export function LlmCard({ form, set, settings, testing, testResult, onTest }: Props) {
  return (
    <Card title="模型服务（LLM）" className="settings-card">
      <Field label="Endpoint">
        <Input
          value={form.endpoint}
          onChange={(v) => set('endpoint', v)}
          placeholder="https://api.openai.com/v1"
        />
      </Field>
      <Field label="模型">
        <Input
          value={form.model}
          onChange={(v) => set('model', v)}
          placeholder="gpt-4o-mini / Qwen / deepseek-v4-flash"
        />
      </Field>
      <Field label="思考等级">
        <Select<string> value={form.effort} onChange={(v) => set('effort', String(v))} style={{ width: '100%' }}>
          <Select.Option value="none">关闭</Select.Option>
          <Select.Option value="low">低</Select.Option>
          <Select.Option value="medium">中</Select.Option>
          <Select.Option value="high">高</Select.Option>
          <Select.Option value="xhigh">超高</Select.Option>
          <Select.Option value="max">最高</Select.Option>
        </Select>
      </Field>
      <Field label={<span>API Key <ConfiguredTag configured={settings.llm.apiKeyConfigured} /></span>}>
        <Input
          mode="password"
          value={form.apiKey}
          onChange={(v) => set('apiKey', v)}
          placeholder={settings.llm.apiKeyConfigured ? '已配置，留空不修改' : '粘贴 API Key'}
        />
      </Field>
      <div className="settings-field-inline">
        <div className="settings-field">
          <span className="settings-label">Temperature</span>
          <InputNumber
            value={form.temperature ? Number(form.temperature) : undefined}
            onChange={(v) => set('temperature', v == null ? '' : String(v))}
            min={0}
            max={2}
            step={0.1}
            placeholder="默认 0.7"
            style={{ width: '100%' }}
          />
        </div>
        <div className="settings-field">
          <span className="settings-label">单次输出上限 (tokens)</span>
          <InputNumber
            value={form.maxTokens ? Number(form.maxTokens) : undefined}
            onChange={(v) => set('maxTokens', v == null ? '' : String(v))}
            min={256}
            max={131072}
            step={1024}
            placeholder="默认 65536"
            style={{ width: '100%' }}
          />
        </div>
        <div className="settings-field">
          <span className="settings-label">上下文窗口 (tokens)</span>
          <InputNumber
            value={form.contextWindow ? Number(form.contextWindow) : undefined}
            onChange={(v) => set('contextWindow', v == null ? '' : String(v))}
            min={1024}
            max={2000000}
            step={1024}
            placeholder="默认 262144"
            style={{ width: '100%' }}
          />
        </div>
      </div>
      <Text type="tertiary" size="small">
        Temperature 与输出上限只影响新开的工单。**上下文窗口是硬预算**：它是一次会话
        能装下的 token 总数（系统提示 + 全部历史 + 本轮输出）。接近它时 Altas 会压缩历史，
        压完还放不下就发不出消息——所以这个数要填模型的真实值。
      </Text>
      <div className="settings-actions">
        <Button size="small" loading={testing} onClick={onTest}>
          测试连接
        </Button>
        {testResult && (
          <Text size="small" type={testResult.ok ? 'success' : 'danger'}>
            {testResult.text}
          </Text>
        )}
      </div>
    </Card>
  )
}
