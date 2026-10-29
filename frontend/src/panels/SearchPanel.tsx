import { useDeferredValue, useState } from 'react'
import { Empty, Input, List, Skeleton, Tag } from 'antd'
import { SearchOutlined } from '@ant-design/icons'
import { useQuery } from '@tanstack/react-query'

import { api, type SearchHit } from '../api'
import { KIND_LABEL, MEDIUM_LABEL, nodeIcon } from '../nodeVisual'
import { useUi } from '../store'

export default function SearchPanel() {
  const [query, setQuery] = useState('')
  const deferred = useDeferredValue(query.trim())
  const select = useUi((s) => s.select)
  const setView = useUi((s) => s.setView)
  const results = useQuery({
    queryKey: ['search', deferred],
    queryFn: () => api.search(deferred, 30),
    enabled: deferred.length > 0,
  })

  const open = (h: SearchHit) => {
    setView('doc')
    select(h.node_id)
  }

  return (
    <div className="search-page">
      <div className="search-inner">
        <div className="search-head">
          <h1>全文检索</h1>
          <p>标题、别名与定稿正文一起查；中文走归一化、trigram 与 bigram 兜底。</p>
        </div>
        <Input
          autoFocus
          size="large"
          allowClear
          prefix={<SearchOutlined />}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="搜作品名、角色、台词或正文里的一个事实"
        />

        <div className="search-summary">
          {deferred
            ? results.isFetching
              ? '检索中…'
              : `${results.data?.length ?? 0} 条结果`
            : '输入关键词开始'}
        </div>

        {!deferred ? (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="例如：青羽测试、空之轨迹、某个角色的名字" />
        ) : results.isLoading ? (
          <Skeleton active paragraph={{ rows: 6 }} />
        ) : results.isError ? (
          <Empty description={`检索失败：${results.error.message}`} />
        ) : results.data?.length ? (
          <List
            className="search-results"
            dataSource={results.data}
            renderItem={(h) => (
              <List.Item className="search-hit" onClick={() => open(h)}>
                <div className="search-hit-icon">{nodeIcon(h)}</div>
                <div className="search-hit-main">
                  <div className="search-hit-title">
                    {h.title}
                    <Tag>{KIND_LABEL[h.kind]}</Tag>
                    {h.medium ? <Tag>{MEDIUM_LABEL[h.medium]}</Tag> : null}
                  </div>
                  <div className="search-hit-snippet">{h.snippet || '这篇还没有正文'}</div>
                </div>
                <span className="search-hit-score">{Math.round(h.score * 10) / 10}</span>
              </List.Item>
            )}
          />
        ) : (
          <Empty description={`没有找到「${deferred}」`} />
        )}
      </div>
    </div>
  )
}
