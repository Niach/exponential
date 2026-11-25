// #EXP-2133: which readings are an alert, and when one is sent. Pure: the
// clock and the reading come in, the messages to send come out.
import type { MemoryReading } from "./readings"

export interface MemoryThresholds {
  /** Alert while the host's available memory is below this share. */
  hostAvailablePercent: number
  /** Alert while the host's free swap is below this share (a host without swap never). */
  swapFreePercent: number
  /** Alert while a container uses more than this share of its limit. */
  containerUsedPercent: number
  /** Alert while the disk has less than this free; a disk too small to ever hold it (#EXP-2147) never. */
  diskFreeBytes: number
}

export const DEFAULT_THRESHOLDS: MemoryThresholds = { hostAvailablePercent: 12, swapFreePercent: 15, containerUsedPercent: 90, diskFreeBytes: 15 * 1024 ** 3 }

/** Readings in a row (one a minute) before an alert: a build or a backup passes. */
export const CONFIRM_READINGS = 3
/** An alert still standing is said again this often. */
export const REPEAT_MS = 24 * 60 * 60 * 1000

export interface MemoryAlert {
  /** Stable per condition (the diagnostics fingerprint). */
  key: string
  text: string
}

const mb = (bytes: number) => `${Math.round(bytes / 1048576)} MB`
const gb = (bytes: number) => `${(bytes / 1024 ** 3).toFixed(1)} GB`
const percent = (part: number, whole: number) => (part / whole) * 100

/** Every condition the reading meets right now. */
export function memoryAlerts(reading: MemoryReading, thresholds: MemoryThresholds = DEFAULT_THRESHOLDS): MemoryAlert[] {
  const alerts: MemoryAlert[] = []
  const { host } = reading
  if (host && percent(host.availableBytes, host.totalBytes) < thresholds.hostAvailablePercent) {
    alerts.push({
      key: `host-memory`,
      text: `The host has ${mb(host.availableBytes)} of ${mb(host.totalBytes)} memory available`,
    })
  }
  if (host && host.swapTotalBytes > 0 && percent(host.swapFreeBytes, host.swapTotalBytes) < thresholds.swapFreePercent) {
    alerts.push({ key: `host-swap`, text: `The host has ${mb(host.swapFreeBytes)} of ${mb(host.swapTotalBytes)} swap free` })
  }
  const { disk } = reading
  if (disk && disk.totalBytes > 2 * thresholds.diskFreeBytes && disk.freeBytes < thresholds.diskFreeBytes) {
    alerts.push({ key: `host-disk`, text: `The host has ${gb(disk.freeBytes)} of ${gb(disk.totalBytes)} disk free` })
  }
  for (const container of reading.containers) {
    if (container.limitBytes && percent(container.usedBytes, container.limitBytes) > thresholds.containerUsedPercent) {
      alerts.push({
        key: `container:${container.name}`,
        text: `Container ${container.name} uses ${mb(container.usedBytes)} of its ${mb(container.limitBytes)} limit`,
      })
    }
  }
  return alerts
}

/** Turns readings into what to SEND: confirmed, once, repeated daily; an OOM kill at once. */
export class MemoryAlertState {
  private readonly streak = new Map<string, number>()
  private readonly sentAt = new Map<string, number>()
  private kills: Map<string, number> | null = null

  constructor(private readonly thresholds: MemoryThresholds = DEFAULT_THRESHOLDS) {}

  next(reading: MemoryReading, nowMs: number): MemoryAlert[] {
    const send: MemoryAlert[] = []
    const standing = memoryAlerts(reading, this.thresholds)
    const keys = new Set(standing.map((alert) => alert.key))
    for (const key of [...this.streak.keys()]) {
      if (keys.has(key)) continue
      this.streak.delete(key)
      this.sentAt.delete(key)
    }
    for (const alert of standing) {
      const streak = (this.streak.get(alert.key) ?? 0) + 1
      this.streak.set(alert.key, streak)
      const last = this.sentAt.get(alert.key)
      if (streak >= CONFIRM_READINGS && (last === undefined || nowMs - last >= REPEAT_MS)) {
        this.sentAt.set(alert.key, nowMs)
        send.push(alert)
      }
    }
    // The first reading is the baseline: kills before this process started are history.
    const kills = new Map(reading.containers.map((container) => [container.name, container.oomKills]))
    for (const [name, count] of this.kills ? kills : []) {
      const before = this.kills!.get(name) ?? 0
      if (count > before) {
        send.push({ key: `oom:${name}`, text: `The kernel killed ${count - before} process(es) at the memory limit of container ${name}` })
      }
    }
    this.kills = kills
    return send
  }
}
