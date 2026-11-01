import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { STEER_CONN_STATS, steerConnEventValid, type SteerConnEvent } from "@exp/domain-contract/steer-enet"
import { ConnStatsBuffer, SteerViewStats } from "./conn-stats"

// EXP-1527 (steer-enet.json `connStats`): the web's steer.view events.

beforeEach(() => vi.useFakeTimers())
afterEach(() => vi.useRealTimers())

describe(`SteerViewStats`, () => {
  it(`an ok settle on the first frame and an ended view with its fallback, all valid`, () => {
    const events: SteerConnEvent[] = []
    let now = 1_000
    const view = new SteerViewStats((e) => events.push(e), () => now)
    view.start()
    view.failure(`join_timeout`)
    view.fallback()
    now += 2_900
    view.applied(`server_fallback`, true)
    view.applied(`iroh_relay`, true)
    now += 60_000
    view.end()
    expect(events.every(steerConnEventValid)).toBe(true)
    expect(events.map((e) => e.data)).toEqual([
      expect.objectContaining({ phase: `settled`, platform: `web`, outcome: `ok`, path: `server_fallback`, firstFrameMs: 2_900 }),
      expect.objectContaining({
        phase: `ended`,
        outcome: `ok`,
        path: `server_fallback`,
        durationMs: 62_900,
        fallbacks: 1,
        reconnects: 1,
        failures: { join_timeout: 1 },
      }),
    ])
    expect(JSON.stringify(events)).not.toContain(`session`)
  })

  it(`no frame within settleAfterMs = a failed settle without a path`, () => {
    const events: SteerConnEvent[] = []
    const view = new SteerViewStats((e) => events.push(e))
    view.start()
    view.failure(`device_offline`)
    vi.advanceTimersByTime(STEER_CONN_STATS.settleAfterMs)
    expect(events).toHaveLength(1)
    expect(steerConnEventValid(events[0])).toBe(true)
    expect(events[0].data).toMatchObject({ phase: `settled`, outcome: `failed`, errorClass: `device_offline` })
    expect(events[0].data.path).toBeUndefined()
  })
})

describe(`ConnStatsBuffer`, () => {
  it(`posts in batches of batchMax every flushMs, drops invalid events, never throws`, async () => {
    const posted: number[] = []
    const buffer = new ConnStatsBuffer(async (events) => {
      posted.push(events.length)
      throw new Error(`server down`)
    })
    const view = (i: number): SteerConnEvent => ({
      v: 1,
      id: `e${i}`,
      kind: `steer.view`,
      at: 1_793_500_000_000,
      source: `steer-viewer`,
      data: { phase: `settled`, platform: `web`, version: `1`, outcome: `ok`, path: `iroh_relay`, firstFrameMs: 5 },
    })
    for (let i = 0; i < STEER_CONN_STATS.batchMax + 3; i++) buffer.record(view(i))
    buffer.record({ ...view(99), sessionId: `s1` } as SteerConnEvent)
    expect(buffer.size).toBe(STEER_CONN_STATS.batchMax + 3)
    await vi.advanceTimersByTimeAsync(STEER_CONN_STATS.flushMs)
    expect(posted).toEqual([STEER_CONN_STATS.batchMax, 3])
    expect(buffer.size).toBe(0)
  })
})
