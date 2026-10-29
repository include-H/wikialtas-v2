import type { ReactNode } from 'react'
import {
  AccountBookOutlined,
  BookOutlined,
  DesktopOutlined,
  FileOutlined,
  FileTextOutlined,
  FolderOutlined,
  GlobalOutlined,
  PlayCircleOutlined,
  ReadOutlined,
  VideoCameraOutlined,
} from '@ant-design/icons'

import type { Medium, Node, NodeKind, NodeStatus } from './api'

export const KIND_LABEL: Record<NodeKind, string> = {
  universe: '宇宙',
  series: '系列',
  work: '作品',
  doc: '资料',
}

export const MEDIUM_LABEL: Record<Medium, string> = {
  game: '游戏',
  movie: '电影',
  tv: '剧集',
  manga: '漫画',
  book: '图书',
  other: '其他',
}

type NodeLike = Pick<Node, 'kind' | 'medium'> & { tags?: string[] }

/** 树节点图标按种类 / 介质给（`design.md §6.2`）。 */
export function nodeIcon(n: NodeLike): ReactNode {
  if (n.kind === 'doc') {
    return n.tags?.includes('账本') ? <AccountBookOutlined /> : <FileTextOutlined />
  }
  if (n.kind === 'universe') return <GlobalOutlined />
  if (n.kind === 'series') return <FolderOutlined />
  switch (n.medium) {
    case 'game':
      return <PlayCircleOutlined />
    case 'movie':
      return <VideoCameraOutlined />
    case 'tv':
      return <DesktopOutlined />
    case 'manga':
      return <ReadOutlined />
    case 'book':
      return <BookOutlined />
    default:
      return <FileOutlined />
  }
}

/** 状态点：草稿=橙点、定稿=无点、归档=灰字（`design.md §6.2`）。 */
export function statusClass(status: NodeStatus): string {
  if (status === 'draft') return 'is-draft'
  if (status === 'archived') return 'is-archived'
  return 'is-ready'
}

/** 一行 meta 的中文标签，如「游戏 · 作品」。 */
export function metaLabel(kind: NodeKind, medium: Medium | null): string {
  const parts: string[] = []
  if (medium) parts.push(MEDIUM_LABEL[medium])
  parts.push(KIND_LABEL[kind])
  return parts.join(' · ')
}
