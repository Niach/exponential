import { describe, expect, it } from "vitest"
import {
  classifyRequestPath,
  metricsSnapshot,
  recordNotificationFanout,
  recordShapeRequest,
  reportSchedulerRun,
} from "./registry"

// The registry is module-singleton state shared across this file's tests, so
// each test uses its own scheduler names / tables and asserts deltas, not
// absolute totals.

function classify(path: string, query = ``): string {
  return classifyRequestPath(path, new URLSearchParams(query))
}

describe(`classifyRequestPath`, () => {
  it(`splits shape requests on the affirmative live params`, () => {
    expect(classify(`/api/shapes/issues`, `live=true`)).toBe(`shape-live`)
    expect(classify(`/api/shapes/issues`, `live_sse=true`)).toBe(`shape-live`)
    expect(classify(`/api/shapes/issues`, `experimental_live_sse=true`)).toBe(
      `shape-live`
    )
    // Value check, not presence: live=false is snapshot-class (matches
    // electric-proxy's isLiveRequest).
    expect(classify(`/api/shapes/issues`, `live=false`)).toBe(`shape-snapshot`)
    expect(classify(`/api/shapes/issues`, `offset=-1`)).toBe(`shape-snapshot`)
  })

  it(`classifies trpc and other api routes`, () => {
    expect(classify(`/api/trpc/issues.update`)).toBe(`trpc`)
    expect(classify(`/api/health`)).toBe(`api-other`)
    expect(classify(`/api/attachments/abc`)).toBe(`api-other`)
  })

  it(`classifies assets by prefix and extension`, () => {
    expect(classify(`/assets/index-abc123.js`)).toBe(`asset`)
    expect(classify(`/widget/v1/loader.js`)).toBe(`asset`)
    expect(classify(`/favicon.ico`)).toBe(`asset`)
  })

  it(`falls back to app for navigations`, () => {
    expect(classify(`/`)).toBe(`app`)
    expect(classify(`/t/acme/boards/core/issues/ACME-12`)).toBe(`app`)
  })
})

describe(`reportSchedulerRun`, () => {
  it(`accumulates runs and failures and keeps the last error`, () => {
    reportSchedulerRun(`test-sched-a`, { ok: true, durationMs: 5, detail: `2 sent` })
    reportSchedulerRun(`test-sched-a`, { ok: false, durationMs: 9, error: `boom` })
    reportSchedulerRun(`test-sched-a`, { ok: true, durationMs: 3 })

    const entry = metricsSnapshot().schedulers.find(
      (s) => s.name === `test-sched-a`
    )
    expect(entry).toMatchObject({
      runs: 3,
      failures: 1,
      lastOk: true,
      lastDurationMs: 3,
      lastDetail: null,
    })
  })
})

describe(`metricsSnapshot`, () => {
  it(`aggregates per-table shape counters`, () => {
    recordShapeRequest(`test_table_x`, `live`, 200, 12)
    recordShapeRequest(`test_table_x`, `live`, 200, 8)
    recordShapeRequest(`test_table_x`, `snapshot`, 200, 40)

    const entry = metricsSnapshot().electric.perTable.find(
      (t) => t.table === `test_table_x`
    )
    expect(entry).toEqual({ table: `test_table_x`, live: 2, snapshot: 1 })
  })

  it(`derives the deduped notification count`, () => {
    const before = metricsSnapshot().notifications
    recordNotificationFanout({ requested: 5, inserted: 3 })
    const after = metricsSnapshot().notifications
    expect(after.fanouts - before.fanouts).toBe(1)
    expect(after.deduped - before.deduped).toBe(2)
  })

  it(`is JSON-round-trippable`, () => {
    const snapshot = metricsSnapshot()
    expect(JSON.parse(JSON.stringify(snapshot))).toEqual(snapshot)
  })
})
