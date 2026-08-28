// Delayed capture (FEED-18): the hold ticks whole seconds down to 0, and the
// 0 tick lands BEFORE the promise resolves with a frame + settle in between,
// so the caller's countdown UI is off screen when the engine grabs the shot.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { runCountdown } from "./countdown"

describe(`runCountdown`, () => {
  beforeEach(() => {
    vi.useFakeTimers()
    if (typeof globalThis.requestAnimationFrame !== `function`) {
      globalThis.requestAnimationFrame = (fn: FrameRequestCallback) => {
        setTimeout(() => fn(0), 0)
        return 0
      }
    }
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it(`resolves immediately without ticks when there is no delay`, async () => {
    const onTick = vi.fn()
    let settled = false
    void runCountdown(0, onTick).then(() => {
      settled = true
    })
    await vi.advanceTimersByTimeAsync(0)
    expect(settled).toBe(true)
    expect(onTick).not.toHaveBeenCalled()
  })

  it(`ticks 3, 2, 1 once per second, then 0 before resolving`, async () => {
    const onTick = vi.fn()
    let settled = false
    void runCountdown(3_000, onTick).then(() => {
      settled = true
    })
    expect(onTick.mock.calls.map(([n]) => n)).toEqual([3])
    await vi.advanceTimersByTimeAsync(1_000)
    expect(onTick.mock.calls.map(([n]) => n)).toEqual([3, 2])
    await vi.advanceTimersByTimeAsync(1_000)
    expect(onTick.mock.calls.map(([n]) => n)).toEqual([3, 2, 1])
    await vi.advanceTimersByTimeAsync(1_000)
    expect(onTick.mock.calls.map(([n]) => n)).toEqual([3, 2, 1, 0])
    // The 0 tick has fired but the frame + settle wait is still pending.
    expect(settled).toBe(false)
    await vi.advanceTimersByTimeAsync(200)
    expect(settled).toBe(true)
  })

  it(`rounds a fractional delay up and shortens the first step`, async () => {
    const onTick = vi.fn()
    void runCountdown(2_500, onTick)
    expect(onTick).toHaveBeenLastCalledWith(3)
    await vi.advanceTimersByTimeAsync(500)
    expect(onTick).toHaveBeenLastCalledWith(2)
    await vi.advanceTimersByTimeAsync(1_000)
    expect(onTick).toHaveBeenLastCalledWith(1)
    await vi.advanceTimersByTimeAsync(1_000)
    expect(onTick).toHaveBeenLastCalledWith(0)
  })
})
