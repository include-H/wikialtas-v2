import { useCallback, useEffect, useRef, useState } from 'react'
import { Toast } from '@douyinfe/semi-ui'
import type { NodeLibraryResponse, NodePendingItem } from '../types'
import {
  archiveLibrary,
  dismissNodeSweep,
  getNodeLibrary,
  getWorkRelations,
  linkLibraryUnified,
  restoreNodeSweep,
  runNodeSweep,
  setLinkRole,
  unlinkEmbyLink,
  unlinkGameAtlas,
  unlinkKomgaLink,
} from './api'
import type { LibraryApiSource } from './api'
import type { Relation } from '../types'
import { useAppStore } from './store'

/**
 * 作品页「关联与媒体」的唯一数据源。
 *
 * 一个 `getNodeLibrary` 端点吐齐正片级链 / 混杂链 / 待确认 / 已忽略 / 源状态
 * （替代旧的五连发:getWork×2 + sweep + related + suggest×3）;关系边低频单独拉。
 * 去重不再靠标题启发式——后端建议 KV 里同一个 key 只有一条,交集天然为空。
 *
 * 写完之后后端自动扫库(判官十几秒):这里轻轮询(5s)直到待确认出现,90s 收手。
 */
export function useWorkLibrary(workId: string | null) {
  const { me, nodes, lastCommitted, refreshTree, setPendingCount } = useAppStore()
  const [data, setData] = useState<NodeLibraryResponse | null>(null)
  const [relations, setRelations] = useState<Relation[]>([])
  const [loading, setLoading] = useState(!!workId)
  const [error, setError] = useState<string | null>(null)
  const [watching, setWatching] = useState(false)
  // 动作中的行 key:待确认行用建议 key;链行用链接 id
  const [busyKey, setBusyKey] = useState<string | null>(null)
  const seqRef = useRef(0)
  // 路由竞态:慢请求回来时如果 workId 已经变了,丢弃
  const workIdRef = useRef(workId)
  workIdRef.current = workId

  const load = useCallback(
    async (opts?: { silent?: boolean }) => {
      if (!workId) return
      const requested = workId
      const seq = ++seqRef.current
      if (!opts?.silent) {
        setLoading(true)
        setError(null)
      }
      try {
        const [nd, rr] = await Promise.all([
          getNodeLibrary(requested),
          getWorkRelations(requested).catch(() => null),
        ])
        if (seq !== seqRef.current || workIdRef.current !== requested) return
        setData(nd)
        setRelations(rr?.relations ?? [])
      } catch (e) {
        if (seq !== seqRef.current || workIdRef.current !== requested) return
        if (opts?.silent) return
        setError(e instanceof Error ? e.message : '读取关联与媒体失败')
        setData(null)
      } finally {
        if (seq === seqRef.current && workIdRef.current === requested && !opts?.silent) {
          setLoading(false)
        }
      }
    },
    [workId],
  )

  useEffect(() => {
    setData(null)
    setRelations([])
    setWatching(false)
    setPendingCount(0)
    if (!workId || !me.authed) {
      setLoading(false)
      return
    }
    void load()
  }, [workId, me.authed, load, setPendingCount])

  // 待确认计数上报(拉门角标用)
  useEffect(() => {
    setPendingCount(data?.pending.length ?? 0)
  }, [data, setPendingCount])

  // 写完一波:轻轮询直到待确认出现(后台扫库完才写进 KV)
  useEffect(() => {
    if (!me.authed || !workId || !lastCommitted || lastCommitted.targetId !== workId) return
    if ((data?.pending.length ?? 0) > 0) return
    setWatching(true)
    const done = setTimeout(() => setWatching(false), 90_000)
    return () => clearTimeout(done)
  }, [me.authed, workId, lastCommitted, data])

  useEffect(() => {
    if (!watching || !workId) return
    const iv = setInterval(() => {
      void (async () => {
        try {
          const res = await getNodeLibrary(workId)
          if (workIdRef.current !== workId || res.pending?.length) {
            if (workIdRef.current === workId) {
              setData(res)
              setWatching(false)
            }
          }
        } catch {
          // 轮询失败不提示;下一次再试
        }
      })()
    }, 5000)
    return () => clearInterval(iv)
  }, [watching, workId])

  const reload = useCallback(() => load({ silent: true }), [load])

  /** 确认一条待确认(挂链到本节点)。 */
  const linkPending = useCallback(
    async (s: NodePendingItem) => {
      if (!workId) return
      setBusyKey(s.key)
      try {
        await linkLibraryUnified({
          workId,
          source: s.source as LibraryApiSource,
          publicId: s.entryId,
          origin: s.origin === 'pool' ? 'pool' : 'sweep',
        })
        Toast.success(`已关联「${s.title}」`)
        await reload()
      } catch (e) {
        Toast.error(e instanceof Error ? e.message : '关联失败')
      } finally {
        setBusyKey(null)
      }
    },
    [workId, reload],
  )

  /** 建档:建 stub 子节点并挂链(条目属于同系列另一部作品时)。 */
  const archivePending = useCallback(
    async (s: NodePendingItem) => {
      if (!workId) return
      setBusyKey(s.key)
      try {
        const res = await archiveLibrary(s.source as LibraryApiSource, workId, s.entryId)
        Toast.success(`已建档「${res.work?.title ?? s.title}」`)
        await Promise.all([reload(), refreshTree()])
      } catch (e) {
        Toast.error(e instanceof Error ? e.message : '建档失败')
      } finally {
        setBusyKey(null)
      }
    },
    [workId, reload, refreshTree],
  )

  /** 忽略(per-node):不再向这个节点提起;可附理由(有当儿的会作为反馈喂给「AI 对一遍」)。 */
  const dismissPending = useCallback(
    async (s: NodePendingItem, reason?: string) => {
      if (!workId) return
      setBusyKey(s.key)
      try {
        await dismissNodeSweep(workId, s.key, reason)
        await reload()
      } catch (e) {
        Toast.error(e instanceof Error ? e.message : '忽略失败')
      } finally {
        setBusyKey(null)
      }
    },
    [workId, reload],
  )

  const restoreDismissed = useCallback(
    async (key: string) => {
      if (!workId) return
      setBusyKey(key)
      try {
        await restoreNodeSweep(workId, key)
        await reload()
      } catch (e) {
        Toast.error(e instanceof Error ? e.message : '恢复失败')
      } finally {
        setBusyKey(null)
      }
    },
    [workId, reload],
  )

  /** 手动检索(题名变体 → 三源候选 → 判官;term = 用户附加的检索词)。 */
  const rescan = useCallback(
    async (term?: string) => {
      if (!workId) return
      setBusyKey('__rescan')
      try {
        const res = await runNodeSweep(workId, term?.trim() || undefined)
        const own = res.suggestions.filter(s => s.targetNodeId === workId)
        const other = res.suggestions.filter(s => s.targetNodeId && s.targetNodeId !== workId)
        if (own.length > 0) {
          Toast.info(`扫完:这份节点有 ${own.length} 条建议`)
        } else if (other.length > 0) {
          // 建议都挂到了别节点(条目讲了别的作品):说破,别装没发生
          const names = [
            ...new Set(other.map(s => {
              const n = nodes.find(n => n.id === s.targetNodeId)
              return n ? n.title : s.title
            })),
          ].slice(0, 2)
          Toast.info(`扫完:${other.length} 条条目不属于这份节点,更像属于「${names.join('」「')}」`)
        } else {
          Toast.info(term?.trim() ? `按「${term.trim()}」兜了一圈,没找到能挂上来的条目` : '兜了一圈,没找到属于它的条目')
        }
        await reload()
      } catch (e) {
        Toast.error(e instanceof Error ? e.message : '检索失败')
      } finally {
        setBusyKey(null)
      }
    },
    [workId, reload, nodes],
  )

  /** 解除一条链(游戏整条 / 影视·书籍按链接 id)。 */
  const unlink = useCallback(
    async (linkId: string, source: LibraryApiSource) => {
      if (!workId) return
      setBusyKey(linkId)
      try {
        if (source === 'gameatlas') {
          await unlinkGameAtlas(workId)
        } else if (source === 'emby') {
          await unlinkEmbyLink(linkId)
        } else {
          await unlinkKomgaLink(linkId)
        }
        Toast.success('已解除关联')
        await reload()
      } catch (e) {
        Toast.error(e instanceof Error ? e.message : '解除失败')
      } finally {
        setBusyKey(null)
      }
    },
    [workId, reload],
  )

  /** 改判链的角色(正片级 ↔ 混杂级)。 */
  const relinkRole = useCallback(
    async (linkId: string, role: 'work' | 'mix') => {
      setBusyKey(linkId)
      try {
        await setLinkRole(linkId, role)
        Toast.success(role === 'work' ? '已归到正片级' : '已归到混杂内容')
        await reload()
      } catch (e) {
        Toast.error(e instanceof Error ? e.message : '操作失败')
      } finally {
        setBusyKey(null)
      }
    },
    [reload],
  )

  return {
    data,
    relations,
    loading,
    error,
    watching,
    busyKey,
    reload,
    linkPending,
    archivePending,
    dismissPending,
    restoreDismissed,
    rescan,
    unlink,
    relinkRole,
  }
}
