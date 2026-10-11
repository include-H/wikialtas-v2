import type { PoolEntry } from '../types'

/**
 * 一次批量最多建几篇：建档的下一步是**真花钱写正文**（/stubs → Altas），
 * 所以一次点出几百个工单是糟糕的默认。超出的留在列表里，下次再选。
 */
export const MAX_BATCH = 20

/** 只在这里保留 PoolEntry 的引用，避免调用方 import 循环（纯类型占位）。 */
export type BatchEntry = PoolEntry
