// #EXP-2158: what the web process itself holds, as one log line every ten
// minutes: the JavaScript heap against everything native, and which object
// types grew since the first sample. Type names and numbers only: unlike a
// heap snapshot it contains no data of anybody.
interface HeapStats {
  heapSize: number
  heapCapacity: number
  extraMemorySize: number
  objectCount: number
  objectTypeCounts: Record<string, number>
}

export interface ProcessHeap {
  rssBytes: number
  heapBytes: number
  heapCapacityBytes: number
  /** Memory the heap's objects hold outside it (buffers, strings, streams). */
  extraBytes: number
  objects: number
  types: Record<string, number>
}

/** Null outside Bun. */
export function readProcessHeap(): ProcessHeap | null {
  const load = (process as { getBuiltinModule?: (id: string) => unknown }).getBuiltinModule
  let heapStats: (() => HeapStats) | undefined
  try {
    heapStats = (load?.(`bun:jsc`) as { heapStats?: () => HeapStats } | undefined)?.heapStats
  } catch {
    return null
  }
  if (!heapStats) return null
  const stats = heapStats()
  return {
    rssBytes: process.memoryUsage.rss(),
    heapBytes: stats.heapSize,
    heapCapacityBytes: stats.heapCapacity,
    extraBytes: stats.extraMemorySize,
    objects: stats.objectCount,
    types: stats.objectTypeCounts,
  }
}

const mb = (bytes: number) => Math.round(bytes / 1048576)
const TOP_TYPES = 12

/** One line; the types are the ones that grew most since `first`, as `name count (+growth)`. */
export function processHeapLine(now: ProcessHeap, first: ProcessHeap): string {
  const grown = Object.entries(now.types)
    .map(([name, count]) => ({ name, count, growth: count - (first.types[name] ?? 0) }))
    .filter((type) => type.growth > 0)
    .sort((a, b) => b.growth - a.growth)
    .slice(0, TOP_TYPES)
    .map((type) => `${type.name} ${type.count} (+${type.growth})`)
  return (
    `rss ${mb(now.rssBytes)} MB, heap ${mb(now.heapBytes)}/${mb(now.heapCapacityBytes)} MB, ` +
    `extra ${mb(now.extraBytes)} MB, ${now.objects} objects` +
    (grown.length > 0 ? `; grown: ${grown.join(`, `)}` : ``)
  )
}
