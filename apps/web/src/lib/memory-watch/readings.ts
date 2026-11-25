// #EXP-2133: what the memory watch reads. Plain files, no shell, no Docker
// socket: /proc/meminfo shows the HOST inside a container, /sys/fs/cgroup shows
// this container, and a read-only bind of the host's /sys/fs/cgroup
// (MEMORY_WATCH_CGROUP_ROOT) shows every container on the host. The disk is the
// one this container's filesystem lies on: Docker's, on the host.
import { readFile, readdir, statfs } from "node:fs/promises"
import { join } from "node:path"

export interface HostMemory {
  totalBytes: number
  availableBytes: number
  swapTotalBytes: number
  swapFreeBytes: number
}

export interface ContainerMemory {
  /** The container id's first 12 characters (`docker ps`), or `this web container`. */
  name: string
  /** Without the reclaimable file cache: what an OOM kill would count. */
  usedBytes: number
  /** null = no limit. */
  limitBytes: number | null
  /** Processes the kernel killed at this container's limit since it started. */
  oomKills: number
}

export interface DiskSpace {
  totalBytes: number
  freeBytes: number
}

export interface MemoryReading {
  host: HostMemory | null
  disk: DiskSpace | null
  containers: ContainerMemory[]
}

export function parseMeminfo(text: string): HostMemory | null {
  const kb = (key: string): number | null => {
    const match = new RegExp(`^${key}:\\s+(\\d+) kB$`, `m`).exec(text)
    return match ? Number(match[1]) * 1024 : null
  }
  const totalBytes = kb(`MemTotal`)
  const availableBytes = kb(`MemAvailable`)
  if (!totalBytes || availableBytes === null) return null
  return { totalBytes, availableBytes, swapTotalBytes: kb(`SwapTotal`) ?? 0, swapFreeBytes: kb(`SwapFree`) ?? 0 }
}

const counter = (text: string | null, key: string): number => {
  const match = text === null ? null : new RegExp(`^${key} (\\d+)$`, `m`).exec(text)
  return match ? Number(match[1]) : 0
}

/** One cgroup v2 directory, or null when it is none (cgroup v1, the root, a container that just stopped). */
async function readCgroup(dir: string, name: string): Promise<ContainerMemory | null> {
  const read = (file: string) => readFile(join(dir, file), `utf8`).catch(() => null)
  const [current, max, stat, events] = await Promise.all([
    read(`memory.current`),
    read(`memory.max`),
    read(`memory.stat`),
    read(`memory.events`),
  ])
  if (current === null || max === null) return null
  const cache = counter(stat, `inactive_file`) + counter(stat, `active_file`)
  const limit = max.trim() === `max` ? null : Number(max)
  return {
    name,
    usedBytes: Math.max(0, Number(current) - cache),
    limitBytes: limit !== null && Number.isFinite(limit) ? limit : null,
    oomKills: counter(events, `oom_kill`),
  }
}

/** Docker's cgroups below a host cgroup root, for both of its drivers. */
async function readContainers(root: string): Promise<ContainerMemory[]> {
  const found: Array<{ dir: string; id: string }> = []
  for (const [parent, pattern] of [
    [`system.slice`, /^docker-([0-9a-f]{12,64})\.scope$/],
    [`docker`, /^([0-9a-f]{12,64})$/],
  ] as const) {
    for (const entry of await readdir(join(root, parent)).catch(() => [])) {
      const id = pattern.exec(entry)?.[1]
      if (id) found.push({ dir: join(root, parent, entry), id })
    }
  }
  const all = await Promise.all(found.map(({ dir, id }) => readCgroup(dir, id.slice(0, 12))))
  return all.filter((container) => container !== null)
}

export interface ReadingPaths {
  meminfo?: string
  ownCgroup?: string
  /** The host's cgroup root, bind-mounted; unset = only this container. */
  cgroupRoot?: string
  disk?: string
}

async function readDisk(path: string): Promise<DiskSpace | null> {
  try {
    const { bsize, blocks, bavail } = await statfs(path)
    return blocks > 0 ? { totalBytes: bsize * blocks, freeBytes: bsize * bavail } : null
  } catch {
    return null
  }
}

export async function readMemory(paths: ReadingPaths = {}): Promise<MemoryReading> {
  const meminfo = await readFile(paths.meminfo ?? `/proc/meminfo`, `utf8`).catch(() => null)
  let containers = paths.cgroupRoot ? await readContainers(paths.cgroupRoot) : []
  if (containers.length === 0) {
    const own = await readCgroup(paths.ownCgroup ?? `/sys/fs/cgroup`, `this web container`)
    containers = own ? [own] : []
  }
  return { host: meminfo === null ? null : parseMeminfo(meminfo), disk: await readDisk(paths.disk ?? `/`), containers }
}
