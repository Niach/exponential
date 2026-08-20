// In-process metrics registry for the admin performance page (EXP-553).
// Zero dependencies, module-singleton state, O(1) hot-path writes. Everything
// resets on process restart by design — the page says so. SERVER-ONLY: this
// module must never be imported from client components (it would drag
// process-global state into the bundle for nothing).

import { MinuteRing } from "@/lib/metrics/ring"

// ── Request classes ───────────────────────────────────────────────────────────

export type RequestClass =
  | `shape-live`
  | `shape-snapshot`
  | `trpc`
  | `api-other`
  | `asset`
  | `app`

export const REQUEST_CLASSES: RequestClass[] = [
  `shape-live`,
  `shape-snapshot`,
  `trpc`,
  `api-other`,
  `asset`,
  `app`,
]

// Affirmative-value check mirroring electric-proxy's isLiveRequest — on the
// INBOUND url (the forwarded params are copied verbatim, so the verdicts
// agree).
function isLiveShapeUrl(searchParams: URLSearchParams): boolean {
  return (
    searchParams.get(`live`) === `true` ||
    searchParams.get(`live_sse`) === `true` ||
    searchParams.get(`experimental_live_sse`) === `true`
  )
}

const EXTENSION_RE = /\.[a-zA-Z0-9]+$/

/** Pure classifier for the global request-timing hook in server-bun.ts. */
export function classifyRequestPath(
  pathname: string,
  searchParams: URLSearchParams
): RequestClass {
  if (pathname.startsWith(`/api/shapes/`)) {
    return isLiveShapeUrl(searchParams) ? `shape-live` : `shape-snapshot`
  }
  if (pathname === `/api/trpc` || pathname.startsWith(`/api/trpc/`)) {
    return `trpc`
  }
  if (pathname.startsWith(`/api/`)) return `api-other`
  if (
    pathname.startsWith(`/assets/`) ||
    pathname.startsWith(`/widget/`) ||
    pathname.startsWith(`/_serverFn`) ||
    EXTENSION_RE.test(pathname)
  ) {
    return `asset`
  }
  return `app`
}

// ── State ─────────────────────────────────────────────────────────────────────

const WINDOW_MINUTES = 60

const startedAtMs = Date.now()

const requestRings = new Map<RequestClass, MinuteRing>(
  REQUEST_CLASSES.map((cls) => [cls, new MinuteRing(WINDOW_MINUTES)])
)

// Snapshot-class upstream latency only — a live long-poll blocks ~20s by
// design and would drown the signal.
const electricUpstreamRing = new MinuteRing(WINDOW_MINUTES)
const electricErrorRing = new MinuteRing(WINDOW_MINUTES)
const snapshotWaitRing = new MinuteRing(WINDOW_MINUTES)
// count carries lag samples, maxMs the worst lag seen that minute.
const eventLoopRing = new MinuteRing(WINDOW_MINUTES)

const electricStatus = { s2xx: 0, s4xx: 0, s5xx: 0, aborted499: 0 }
const electricBytes = { rawBytes: 0, sentBytes: 0, gzippedResponses: 0 }
let electricGaugeSource: (() => { active: number; queued: number }) | null =
  null
let electricCapacity = 0
const electricHighWater = { active: 0, queued: 0 }

const shapeTableCounters = new Map<string, { live: number; snapshot: number }>()

interface SchedulerRunInfo {
  lastRunAtMs: number
  lastDurationMs: number
  lastOk: boolean
  lastDetail: string | null
  lastError: string | null
  runs: number
  failures: number
}
const schedulerRuns = new Map<string, SchedulerRunInfo>()

const notifyInfo = { fanouts: 0, requested: 0, inserted: 0 }
const pushInfo = {
  batches: 0,
  requestsOk: 0,
  requestsFailed: 0,
  invalidTokens: 0,
  lastErrorAtMs: null as number | null,
  lastError: null as string | null,
}

// ── Event-loop lag sampler ────────────────────────────────────────────────────

// Drift sampler instead of perf_hooks.monitorEventLoopDelay — the histogram
// API is Node-internal and not reliably implemented in Bun. Started lazily on
// first use so it also runs under `vite dev`, where server-bun.ts never
// executes.
const LAG_SAMPLE_INTERVAL_MS = 500
let samplerStarted = false
let lastLagMs = 0

function ensureSamplerStarted(): void {
  if (samplerStarted) return
  samplerStarted = true
  let expected = Date.now() + LAG_SAMPLE_INTERVAL_MS
  const timer = setInterval(() => {
    const now = Date.now()
    const lag = Math.max(0, now - expected)
    lastLagMs = lag
    eventLoopRing.record(now, lag)
    expected = now + LAG_SAMPLE_INTERVAL_MS
  }, LAG_SAMPLE_INTERVAL_MS)
  timer.unref?.()
}

// ── CPU sampling ──────────────────────────────────────────────────────────────

let lastCpu: { usage: NodeJS.CpuUsage; atMs: number } | null = null
let lastCpuPercent = 0

/** Process CPU as % of one core since the previous call (min 1s apart —
 * closer calls get the cached value, so two polling admins don't zero each
 * other's delta). First call returns 0 and seeds the baseline. */
export function cpuPercentSince(nowMs = Date.now()): number {
  if (lastCpu && nowMs - lastCpu.atMs < 1_000) return lastCpuPercent
  const usage = process.cpuUsage()
  if (lastCpu) {
    const elapsedUs = (nowMs - lastCpu.atMs) * 1_000
    const spentUs =
      usage.user -
      lastCpu.usage.user +
      (usage.system - lastCpu.usage.system)
    lastCpuPercent = elapsedUs > 0 ? (spentUs / elapsedUs) * 100 : 0
  }
  lastCpu = { usage, atMs: nowMs }
  return lastCpuPercent
}

// ── Recording API ─────────────────────────────────────────────────────────────

export function recordRequest(cls: RequestClass, ms: number): void {
  ensureSamplerStarted()
  requestRings.get(cls)?.record(Date.now(), ms)
}

export function recordShapeRequest(
  table: string,
  kind: `live` | `snapshot`,
  _status: number,
  _ms: number
): void {
  let entry = shapeTableCounters.get(table)
  if (!entry) {
    entry = { live: 0, snapshot: 0 }
    shapeTableCounters.set(table, entry)
  }
  entry[kind] += 1
}

export function recordElectricUpstream(args: {
  live: boolean
  status: number
  ms: number
}): void {
  ensureSamplerStarted()
  const now = Date.now()
  if (args.status === 499) electricStatus.aborted499 += 1
  else if (args.status >= 500) electricStatus.s5xx += 1
  else if (args.status >= 400) electricStatus.s4xx += 1
  else electricStatus.s2xx += 1
  if (args.status >= 500) electricErrorRing.record(now)
  if (!args.live) electricUpstreamRing.record(now, args.ms)
}

export function recordElectricBytes(args: {
  rawBytes: number
  sentBytes: number
  gzipped: boolean
}): void {
  electricBytes.rawBytes += args.rawBytes
  electricBytes.sentBytes += args.sentBytes
  if (args.gzipped) electricBytes.gzippedResponses += 1
}

export function recordSnapshotQueueWait(waitMs: number): void {
  snapshotWaitRing.record(Date.now(), waitMs)
}

export function noteElectricHighWater(active: number, queued: number): void {
  if (active > electricHighWater.active) electricHighWater.active = active
  if (queued > electricHighWater.queued) electricHighWater.queued = queued
}

/** electric-proxy registers its live semaphore gauges here at module load. */
export function registerElectricProxyGauges(
  source: () => { active: number; queued: number },
  capacity: number
): void {
  electricGaugeSource = source
  electricCapacity = capacity
}

export function reportSchedulerRun(
  name: string,
  run: { ok: boolean; durationMs: number; detail?: string; error?: string }
): void {
  const prev = schedulerRuns.get(name)
  schedulerRuns.set(name, {
    lastRunAtMs: Date.now(),
    lastDurationMs: run.durationMs,
    lastOk: run.ok,
    lastDetail: run.detail ?? null,
    lastError: run.error ?? (run.ok ? null : (prev?.lastError ?? null)),
    runs: (prev?.runs ?? 0) + 1,
    failures: (prev?.failures ?? 0) + (run.ok ? 0 : 1),
  })
}

export function recordNotificationFanout(args: {
  requested: number
  inserted: number
}): void {
  notifyInfo.fanouts += 1
  notifyInfo.requested += args.requested
  notifyInfo.inserted += args.inserted
}

export function recordPushBatch(args: {
  requestsOk: number
  requestsFailed: number
  invalidTokens: number
  lastError?: string
}): void {
  pushInfo.batches += 1
  pushInfo.requestsOk += args.requestsOk
  pushInfo.requestsFailed += args.requestsFailed
  pushInfo.invalidTokens += args.invalidTokens
  if (args.lastError) {
    pushInfo.lastError = args.lastError
    pushInfo.lastErrorAtMs = Date.now()
  }
}

// ── Snapshot ──────────────────────────────────────────────────────────────────

export interface MetricsSnapshot {
  startedAt: string
  requests: {
    cls: RequestClass
    last5m: number
    last60m: number
    avgMs: number
    maxMs: number
    series: { minute: string; count: number }[]
  }[]
  electric: {
    proxy: {
      capacity: number
      active: number
      queued: number
      activeHighWater: number
      queuedHighWater: number
      queueWaits: { count: number; avgMs: number; maxMs: number }
    }
    upstream: {
      snapshotCount60m: number
      snapshotAvgMs: number
      snapshotMaxMs: number
      status2xx: number
      status4xx: number
      status5xx: number
      aborted499: number
      rawBytes: number
      sentBytes: number
      gzippedResponses: number
      errorSeries: { minute: string; count: number }[]
    }
    perTable: { table: string; live: number; snapshot: number }[]
  }
  eventLoop: {
    lastMs: number
    max60mMs: number
    series: { minute: string; maxMs: number }[]
  }
  schedulers: {
    name: string
    lastRunAt: string
    lastDurationMs: number
    lastOk: boolean
    lastDetail: string | null
    lastError: string | null
    runs: number
    failures: number
  }[]
  notifications: {
    fanouts: number
    requested: number
    inserted: number
    deduped: number
  }
  push: {
    batches: number
    requestsOk: number
    requestsFailed: number
    invalidTokens: number
    lastErrorAt: string | null
    lastError: string | null
  }
}

/** The whole in-memory world in one JSON-safe object. */
export function metricsSnapshot(nowMs = Date.now()): MetricsSnapshot {
  ensureSamplerStarted()
  const gauges = electricGaugeSource?.() ?? { active: 0, queued: 0 }
  const upstreamLatency = electricUpstreamRing.latency(nowMs)
  const eventLoopLatency = eventLoopRing.latency(nowMs)
  return {
    startedAt: new Date(startedAtMs).toISOString(),
    requests: REQUEST_CLASSES.map((cls) => {
      const ring = requestRings.get(cls)!
      const latency = ring.latency(nowMs)
      return {
        cls,
        last5m: ring.total(nowMs, 5),
        last60m: latency.count,
        avgMs: latency.avgMs,
        maxMs: latency.maxMs,
        series: ring
          .buckets(nowMs)
          .map(({ minute, count }) => ({ minute, count })),
      }
    }),
    electric: {
      proxy: {
        capacity: electricCapacity,
        active: gauges.active,
        queued: gauges.queued,
        activeHighWater: electricHighWater.active,
        queuedHighWater: electricHighWater.queued,
        queueWaits: snapshotWaitRing.latency(nowMs),
      },
      upstream: {
        snapshotCount60m: upstreamLatency.count,
        snapshotAvgMs: upstreamLatency.avgMs,
        snapshotMaxMs: upstreamLatency.maxMs,
        status2xx: electricStatus.s2xx,
        status4xx: electricStatus.s4xx,
        status5xx: electricStatus.s5xx,
        aborted499: electricStatus.aborted499,
        rawBytes: electricBytes.rawBytes,
        sentBytes: electricBytes.sentBytes,
        gzippedResponses: electricBytes.gzippedResponses,
        errorSeries: electricErrorRing
          .buckets(nowMs)
          .map(({ minute, count }) => ({ minute, count })),
      },
      perTable: [...shapeTableCounters.entries()]
        .map(([table, counts]) => ({ table, ...counts }))
        .sort((a, b) => b.live + b.snapshot - (a.live + a.snapshot)),
    },
    eventLoop: {
      lastMs: lastLagMs,
      max60mMs: eventLoopLatency.maxMs,
      series: eventLoopRing
        .buckets(nowMs)
        .map(({ minute, maxMs }) => ({ minute, maxMs })),
    },
    schedulers: [...schedulerRuns.entries()]
      .map(([name, info]) => ({
        name,
        lastRunAt: new Date(info.lastRunAtMs).toISOString(),
        lastDurationMs: info.lastDurationMs,
        lastOk: info.lastOk,
        lastDetail: info.lastDetail,
        lastError: info.lastError,
        runs: info.runs,
        failures: info.failures,
      }))
      .sort((a, b) => a.name.localeCompare(b.name)),
    notifications: {
      ...notifyInfo,
      deduped: notifyInfo.requested - notifyInfo.inserted,
    },
    push: {
      batches: pushInfo.batches,
      requestsOk: pushInfo.requestsOk,
      requestsFailed: pushInfo.requestsFailed,
      invalidTokens: pushInfo.invalidTokens,
      lastErrorAt: pushInfo.lastErrorAtMs
        ? new Date(pushInfo.lastErrorAtMs).toISOString()
        : null,
      lastError: pushInfo.lastError,
    },
  }
}
