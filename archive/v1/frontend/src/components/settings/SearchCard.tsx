import { Card, Input } from '@douyinfe/semi-ui'
import { ConfiguredTag, Field, type SectionProps } from './common'

export function SearchCard({ form, set, settings }: SectionProps) {
  return (
    <Card title="联网检索（Exa）" className="settings-card">
      <Field
        label={<span>Exa API Key <ConfiguredTag configured={settings.search.exaApiKeyConfigured} /></span>}
        hint="Altas 用它核实发行日期、销量、获奖这类外部事实；结果按 query 缓存 30 天。"
      >
        <Input
          mode="password"
          value={form.exaApiKey}
          onChange={(v) => set('exaApiKey', v)}
          placeholder={
            settings.search.exaApiKeyConfigured
              ? '已配置，留空不修改'
              : '未配置时 Altas 只用站内知识，事实标「待核实」'
          }
        />
      </Field>
      <Field
        label="网络代理（出外网用）"
        hint="fetch_url 抓单个网页时会直连；直连超时 Altas 会自动带 useProxy 用这个代理重试同一页。检索（Exa）走自己的通道，不受这里影响。"
      >
        <Input
          value={form.proxyUrl}
          onChange={(v) => set('proxyUrl', v)}
          placeholder="http://192.168.1.253:7890（留空 = 不走代理）"
        />
      </Field>
    </Card>
  )
}
