import type { ReactNode } from 'react'
import { Empty, Skeleton, Tooltip } from 'antd'
import { useQuery } from '@tanstack/react-query'
import {
  CheckCircleOutlined,
  DatabaseOutlined,
  EditOutlined,
  FileTextOutlined,
} from '@ant-design/icons'

import { api, type RecentNode } from '../api'
import { KIND_LABEL, MEDIUM_LABEL, nodeIcon } from '../nodeVisual'
import { useUi } from '../store'

/**
 * 首页（`docs/design.md §6.1`）：没选节点时主区就是它。居中 880px：
 * 问候 → 四张统计卡（未建档 → 池子）→ 最近更新。数据来自 `GET /api/overview`（只读聚合）。
 */
export default function HomePanel() {
  const select = useUi((s) => s.select)
  const setView = useUi((s) => s.setView)
  const stats = useQuery({ queryKey: ['overview'], queryFn: api.overview })

  return (
    <div className="home">
      <div className="home-inner">
        <div className="home-hero">
          <span className="home-eyebrow">ATLAS · PERSONAL LIBRARY</span>
          <h1 className="home-hello">{greeting()} · 你的文档库</h1>
          <p>把媒体库里的收藏，整理成一座持续维护的 wiki。</p>
        </div>

        {stats.isLoading ? (
          <Skeleton active paragraph={{ rows: 5 }} />
        ) : stats.isError ? (
          <Empty description="概览没拉到" />
        ) : (
          <>
            <div className="home-cards">
              <StatCard label="作品" value={stats.data!.works} icon={<FileTextOutlined />} />
              <StatCard
                label="已定稿"
                value={stats.data!.committed}
                icon={<CheckCircleOutlined />}
              />
              <StatCard label="草稿中" value={stats.data!.drafting} icon={<EditOutlined />} />
              <StatCard
                label="未建档"
                value={stats.data!.unbuilt}
                icon={<DatabaseOutlined />}
                onClick={() => setView('pool')}
                hint="去媒体库池子挑一部建档"
              />
            </div>

            <div className="home-section-head">
              <span>最近更新</span>
            </div>
            {stats.data!.recent.length === 0 ? (
              <Empty
                description={
                  <span>
                    还空着。去 <a onClick={() => setView('pool')}>媒体库池子</a> 挑一部建档，
                    再让 Atlas 写。
                  </span>
                }
              />
            ) : (
              <ul className="home-recent">
                {stats.data!.recent.map((n) => (
                  <RecentRow key={n.id} n={n} onOpen={() => select(n.id)} />
                ))}
              </ul>
            )}
          </>
        )}
      </div>
    </div>
  )
}

function StatCard({
  label,
  value,
  icon,
  onClick,
  hint,
}: {
  label: string
  value: number
  icon: ReactNode
  onClick?: () => void
  hint?: string
}) {
  const body = (
    <div
      className={`home-card${onClick ? ' is-clickable' : ''}`}
      onClick={onClick}
      role={onClick ? 'button' : undefined}
    >
      <span className="home-card-icon">{icon}</span>
      <div className="home-card-copy">
        <span className="home-card-value">{value}</span>
        <span className="home-card-label">{label}</span>
      </div>
    </div>
  )
  return hint ? <Tooltip title={hint}>{body}</Tooltip> : body
}

function RecentRow({ n, onOpen }: { n: RecentNode; onOpen: () => void }) {
  const status = n.status === 'archived' ? '已归档' : n.has_content ? `定稿 v${n.ver}` : '草稿'
  const meta = [n.medium ? MEDIUM_LABEL[n.medium] : '', KIND_LABEL[n.kind]].filter(Boolean).join(' · ')
  return (
    <li className="home-recent-row" onClick={onOpen}>
      <span className="home-recent-icon">{nodeIcon(n)}</span>
      <span className="home-recent-title">{n.title}</span>
      <span className="home-recent-meta">{meta}</span>
      <span className="home-recent-status">{status}</span>
      <span className="home-recent-time">{relative(n.updated_at)}</span>
    </li>
  )
}

function greeting(): string {
  const h = new Date().getHours()
  if (h < 6) return '夜深了'
  if (h < 12) return '早上好'
  if (h < 18) return '下午好'
  return '晚上好'
}

function relative(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime()
  const min = Math.round(diff / 60_000)
  if (min < 1) return '刚刚'
  if (min < 60) return `${min} 分钟前`
  const hour = Math.round(min / 60)
  if (hour < 24) return `${hour} 小时前`
  return `${Math.round(hour / 24)} 天前`
}
