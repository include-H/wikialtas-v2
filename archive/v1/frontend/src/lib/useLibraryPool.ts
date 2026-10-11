import { useCallback, useEffect, useState } from 'react'
import { Modal, Toast } from '@douyinfe/semi-ui'
import type { LibraryPoolResponse, PoolEntry, PoolGroup } from '../types'
import {
  archivePlace,
  getLibraryPool,
  ignoreLibrary,
  linkLibrary,
  scanLibrary,
  unignoreLibrary,
} from './api'
import type { LibraryApiSource } from './api'
import { indexWikiNodes, matchNode, type PoolSection } from './poolSections'
import { MAX_BATCH } from './poolBatch'
import { useAppStore } from './store'

/**
 * 媒体库建议页的全部状态与动作。
 *
 * 这页是**进料台**：从库里批量挑 → 批量建档（stub + 链）→ 「待写正文」交给 Altas 写。
 * 没有"AI 建议"这个对象（那套随这轮退役）：模型的位置在下游写正文。
 */
export function useLibraryPool() {
  const { me, nodes, refreshTree } = useAppStore()
  const [data, setData] = useState<LibraryPoolResponse | null>(null)
  const [loading, setLoading] = useState(true)
  const [scanning, setScanning] = useState(false)
  const [busy, setBusy] = useState<string | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    try {
      setData(await getLibraryPool())
    } catch (e) {
      Toast.error(e instanceof Error ? e.message : '读取媒体库失败')
      setData(null)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    if (me.authed) void load()
  }, [me.authed, load])

  const itemKey = (e: PoolEntry) => `${e.source}:${e.publicId}`

  const runScan = async () => {
    setScanning(true)
    try {
      const res = await scanLibrary()
      Toast.success(`扫描完成：共 ${res.entries} 条（未挂链 ${res.unlinked}）`)
      await load()
    } catch (e) {
      Toast.error(e instanceof Error ? e.message : '扫描失败')
    } finally {
      setScanning(false)
    }
  }

  const doLink = async (entry: PoolEntry, nodeId: string) => {
    setBusy(itemKey(entry))
    try {
      await linkLibrary(entry.source as LibraryApiSource, nodeId, entry.publicId)
      Toast.success(`已关联「${entry.title}」`)
      await load()
      await refreshTree()
    } catch (e) {
      Toast.error(e instanceof Error ? e.message : '关联失败')
    } finally {
      setBusy(null)
    }
  }

  /**
   * 批量「建为待写」：落位规则全在后端（`domain.Placer`）——
   * 命中单作就关联、命中系列就在其下建子节点、都没有就"建系列+建单作+关联"。
   * 前端只决定"给不给父节点"：
   *   · 容器名对上节点 → parentId = 它
   *   · 作品系列容器（GA/Komga）→ seriesTitle = 容器名（整批挂在同一个系列下）
   *   · 「不归属」→ parentId = ""（顶级）
   *   · 都不给 → 走规则
   */
  const archiveEntries = async (
    entries: PoolEntry[],
    opts?: { parentId?: string; seriesTitle?: string },
  ): Promise<void> => {
    const picked = entries.slice(0, MAX_BATCH)
    if (picked.length < entries.length) {
      Toast.info(`一次最多建 ${MAX_BATCH} 篇，本次先建 ${picked.length} 篇，其余留在列表里`)
    }
    setBusy('bulk:archive')
    try {
      const res = await archivePlace({
        items: picked.map((e) => ({ source: e.source, externalId: e.publicId })),
        ...(opts?.parentId !== undefined ? { parentId: opts.parentId } : {}),
        ...(opts?.seriesTitle ? { seriesTitle: opts.seriesTitle } : {}),
      })
      if (res.failed.length === 0) {
        Toast.success(`已建 ${res.linked} 篇待写（新节点 ${res.created} 个）——去「待写正文」交给 Altas 写`)
      } else {
        Toast.error(
          `已建 ${res.linked} 篇，失败 ${res.failed.length} 篇：` +
            res.failed.slice(0, 3).map((f) => `${f.title || f.externalId}（${f.error}）`).join('；'),
        )
      }
      await load()
      await refreshTree()
    } catch (e) {
      Toast.error(e instanceof Error ? e.message : '建档失败')
    } finally {
      setBusy(null)
    }
  }

  /** 单条建档（界面的「建档到…」）：父节点可空 = 「不归属」。 */
  const doArchive = (entry: PoolEntry, parentId: string) => archiveEntries([entry], { parentId })

  /**
   * 容器级一键。
   *  - 容器名对上节点 → 挂到它下面；
   *  - 作品系列容器（GA/Komga 的 series）→ 用容器名当系列，整组挂进去；
   *  - Emby 的**库**容器（"电影"/"番剧"不是一个作品）→ 返回 false，让界面弹窗问（默认「不归属」）。
   */
  const archiveSection = (section: PoolSection): boolean => {
    if (section.matched) {
      void archiveEntries(section.items, { parentId: section.matched.id })
      return true
    }
    if (section.seriesLike) {
      void archiveEntries(section.items, { seriesTitle: section.group.containerTitle })
      return true
    }
    return false
  }

  /** 把选中里那些"Wiki 已有节点"的条目一次性挂上去。 */
  const linkMany = async (entries: PoolEntry[]) => {
    const index = indexWikiNodes(nodes)
    const jobs: { entry: PoolEntry; nodeId: string }[] = []
    for (const e of entries.slice(0, MAX_BATCH)) {
      const n = matchNode(e.title, index)
      if (n) jobs.push({ entry: e, nodeId: n.id })
    }
    if (jobs.length === 0) {
      Toast.info('选中的条目里没有能在 Wiki 找到对应节点的')
      return
    }
    setBusy('bulk:link')
    let ok = 0
    const failed: string[] = []
    try {
      for (const j of jobs) {
        try {
          await linkLibrary(j.entry.source as LibraryApiSource, j.nodeId, j.entry.publicId)
          ok++
        } catch (e) {
          failed.push(`${j.entry.title}（${e instanceof Error ? e.message : '失败'}）`)
        }
      }
      if (failed.length === 0) Toast.success(`已关联 ${ok} 条`)
      else Toast.error(`已关联 ${ok} 条，失败 ${failed.length} 条：${failed.slice(0, 3).join('；')}`)
      await load()
      await refreshTree()
    } finally {
      setBusy(null)
    }
  }

  const ignoreEntries = async (entries: PoolEntry[]) => {
    setBusy('bulk:ignore')
    let ok = 0
    const failed: string[] = []
    try {
      for (const e of entries) {
        try {
          await ignoreLibrary({ source: e.source, entryId: e.publicId })
          ok++
        } catch (err) {
          failed.push(`${e.title}（${err instanceof Error ? err.message : '失败'}）`)
        }
      }
      if (failed.length === 0) Toast.success(`已忽略 ${ok} 条`)
      else Toast.error(`已忽略 ${ok} 条，失败 ${failed.length} 条`)
      await load()
    } finally {
      setBusy(null)
    }
  }

  const ignoreEntry = async (entry: PoolEntry) => {
    try {
      await ignoreLibrary({ source: entry.source, entryId: entry.publicId })
      Toast.success(`已忽略「${entry.title}」`)
      await load()
    } catch (e) {
      Toast.error(e instanceof Error ? e.message : '忽略失败')
    }
  }

  const ignoreGroup = (group: PoolGroup) => {
    Modal.confirm({
      title: `忽略整组「${group.containerTitle}」？`,
      content: '这一组里的条目将不再出现在这里（可在「已忽略」里恢复）。',
      okText: '忽略整组',
      onOk: async () => {
        try {
          await ignoreLibrary({
            source: group.source,
            containerKey: group.containerKey,
            containerTitle: group.containerTitle,
          })
          Toast.success('已忽略整组')
          await load()
        } catch (e) {
          Toast.error(e instanceof Error ? e.message : '忽略失败')
        }
      },
    })
  }

  const restoreEntry = async (entry: PoolEntry) => {
    try {
      await unignoreLibrary({ source: entry.source, entryId: entry.publicId })
      await load()
    } catch (e) {
      Toast.error(e instanceof Error ? e.message : '恢复失败')
    }
  }

  const restoreGroup = async (source: string, containerKey: string) => {
    try {
      await unignoreLibrary({ source, containerKey })
      await load()
    } catch (e) {
      Toast.error(e instanceof Error ? e.message : '恢复失败')
    }
  }

  return {
    data,
    loading,
    scanning,
    busy,
    itemKey,
    reload: load,
    runScan,
    doArchive,
    doLink,
    archiveSection,
    archiveEntries,
    linkMany,
    ignoreEntries,
    ignoreEntry,
    ignoreGroup,
    restoreEntry,
    restoreGroup,
  }
}
