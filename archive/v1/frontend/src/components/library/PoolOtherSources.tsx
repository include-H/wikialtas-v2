import { useEffect, useState, type ReactNode } from 'react'
import { Button, Typography } from '@douyinfe/semi-ui'
import { IconChevronDown, IconChevronRight } from '@douyinfe/semi-icons'
import type { PoolSection } from '../../lib/poolSections'
import { SOURCE_LABEL } from '../../lib/mediaTags'

const { Text } = Typography

/**
 * ③ 其余：每个来源一行摘要，**默认一个都不展开**。
 * 这一档的意义是"知道还剩多少"，不是"现在就看"；要看就点开，展开后仍是同一套容器卡。
 * 容器卡的渲染由调用方给（`children` 渲染函数）——同一套 props 只在一处装配。
 */
export default function PoolOtherSources({
  restBySource,
  children,
}: {
  restBySource: { source: string; sections: PoolSection[]; count: number }[]
  children: (s: PoolSection) => ReactNode
}) {
  const [open, setOpen] = useState<string | null>(null)
  // 只剩一个来源（比如筛了来源，或别的来源都处理完了）就自动展开——否则整页看着像空的。
  // 用户手动点过之后就不再自动动它。
  const [touched, setTouched] = useState(false)
  useEffect(() => {
    if (!touched && restBySource.length === 1) setOpen(restBySource[0].source)
  }, [restBySource, touched])

  if (restBySource.length === 0) return null

  return (
    <div className="pool-section">
      <Text type="tertiary" size="small" className="pool-section-head">
        其余未挂链（没有现成归处，按来源）
      </Text>
      {restBySource.map(({ source, sections, count }) => (
        <div key={source}>
          <Button
            className="pool-rest-row"
            theme="borderless"
            type="tertiary"
            size="small"
            icon={open === source ? <IconChevronDown /> : <IconChevronRight />}
            onClick={() => {
              setTouched(true)
              setOpen(open === source ? null : source)
            }}
          >
            {SOURCE_LABEL[source] ?? source}
            <Text type="tertiary" size="small" style={{ marginLeft: 8 }}>
              {count} 条未挂链 · {sections.length} 个容器
            </Text>
          </Button>
          {open === source ? sections.map((s) => children(s)) : null}
        </div>
      ))}
    </div>
  )
}
