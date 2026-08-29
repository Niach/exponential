import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  ERROR_STALENESS_WINDOW_MS,
  FAILURE_STREAK_GRACE_MS,
  getConnectivitySnapshot,
  health,
  initialConnectivityState,
  networkUp,
  nextHealthChangeAt,
  recordFailure,
  recordSuccess,
  reportTransportFailure,
  reportTransportResponse,
  reportTransportSuccess,
  resetConnectivityForTests,
  subscribeConnectivity,
  type ConnectivityState,
} from "@/lib/connectivity"

// The pure model is a direct port of desktop `crates/sync/src/health.rs`; the
// seven cases below mirror its test module one for one, so a divergence in
// either client shows up as a failing test on both.

const at = (secs: number) => secs * 1_000
const err = (state: ConnectivityState, secs: number) =>
  recordFailure(state, at(secs), `http 500`)

describe(`connectivity health model`, () => {
  it(`is ok with no error`, () => {
    expect(health(initialConnectivityState(), at(1_000))).toBe(`ok`)
    const s = recordSuccess(initialConnectivityState(), at(500))
    expect(health(s, at(1_000))).toBe(`ok`)
  })

  it(`stays ok while a failure is inside the grace window`, () => {
    const s = err(initialConnectivityState(), 1_000)
    expect(health(s, at(1_005))).toBe(`ok`)
    expect(health(s, at(1_011))).toBe(`ok`)
  })

  it(`goes offline once the streak outlives the grace window`, () => {
    let s = err(initialConnectivityState(), 1_000)
    s = err(s, 1_010)
    expect(health(s, at(1_012))).toBe(`offline`)
  })

  it(`clears instantly on any success after the last error`, () => {
    let s = err(initialConnectivityState(), 1_000)
    s = err(s, 1_020)
    expect(health(s, at(1_020))).toBe(`offline`)
    s = recordSuccess(s, at(1_021))
    expect(health(s, at(1_021))).toBe(`ok`)
  })

  it(`stops alarming once the error itself is stale (the tab was frozen)`, () => {
    let s = err(initialConnectivityState(), 1_000)
    s = err(s, 1_020)
    expect(health(s, at(1_030))).toBe(`offline`)
    expect(health(s, at(1_020 + 300))).toBe(`ok`)
  })

  it(`restarts the streak after a staleness-sized quiet gap`, () => {
    let s = err(initialConnectivityState(), 1_000)
    s = err(s, 1_020)
    // First fresh failure after a suspend-sized gap: a new streak with a fresh
    // grace window, not an instant alarm off the old streak start.
    s = err(s, 1_020 + 400)
    expect(s.failureStreakStartedAt).toBe(at(1_420))
    expect(health(s, at(1_421))).toBe(`ok`)
    s = err(s, 1_430)
    expect(health(s, at(1_433))).toBe(`offline`)
  })

  it(`a success resets the grace for the next failure`, () => {
    let s = err(initialConnectivityState(), 1_000)
    s = err(s, 1_015)
    s = recordSuccess(s, at(1_016))
    s = err(s, 1_017)
    expect(s.failureStreakStartedAt).toBe(at(1_017))
    expect(health(s, at(1_020))).toBe(`ok`)
    expect(health(s, at(1_030))).toBe(`offline`)
  })

  it(`saturates clock skew to zero instead of alarming backwards`, () => {
    const s = err(initialConnectivityState(), 1_000)
    expect(health(s, at(900))).toBe(`ok`)
  })
})

describe(`nextHealthChangeAt`, () => {
  it(`is null when only an event can change the answer`, () => {
    expect(nextHealthChangeAt(initialConnectivityState(), at(1_000))).toBeNull()
    const ok = recordSuccess(err(initialConnectivityState(), 1_000), at(1_001))
    expect(nextHealthChangeAt(ok, at(1_002))).toBeNull()
  })

  it(`points at the end of the grace window while still inside it`, () => {
    const s = err(initialConnectivityState(), 1_000)
    expect(nextHealthChangeAt(s, at(1_001))).toBe(
      at(1_000) + FAILURE_STREAK_GRACE_MS
    )
  })

  it(`points at the staleness edge once already offline`, () => {
    let s = err(initialConnectivityState(), 1_000)
    s = err(s, 1_010)
    expect(nextHealthChangeAt(s, at(1_015))).toBe(
      at(1_010) + ERROR_STALENESS_WINDOW_MS
    )
  })

  it(`is null once the error is already stale`, () => {
    const s = err(initialConnectivityState(), 1_000)
    expect(nextHealthChangeAt(s, at(1_000) + ERROR_STALENESS_WINDOW_MS)).toBeNull()
  })
})

describe(`networkUp`, () => {
  afterEach(() => {
    Object.defineProperty(navigator, `onLine`, {
      configurable: true,
      value: true,
    })
  })

  it(`follows navigator.onLine`, () => {
    expect(networkUp()).toBe(true)
    Object.defineProperty(navigator, `onLine`, {
      configurable: true,
      value: false,
    })
    expect(networkUp()).toBe(false)
  })
})

describe(`the connectivity store`, () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date(at(1_000)))
    resetConnectivityForTests()
  })

  afterEach(() => {
    resetConnectivityForTests()
    vi.useRealTimers()
    Object.defineProperty(navigator, `onLine`, {
      configurable: true,
      value: true,
    })
  })

  it(`flips to offline on its own timer, with no further events`, () => {
    const listener = vi.fn()
    const unsubscribe = subscribeConnectivity(listener)
    reportTransportFailure(new Error(`Failed to fetch`))
    expect(getConnectivitySnapshot()).toBe(`ok`)

    vi.advanceTimersByTime(FAILURE_STREAK_GRACE_MS)
    expect(getConnectivitySnapshot()).toBe(`offline`)
    expect(listener).toHaveBeenCalled()
    unsubscribe()
  })

  it(`clears on the next success`, () => {
    const unsubscribe = subscribeConnectivity(() => {})
    reportTransportFailure(new Error(`Failed to fetch`))
    vi.advanceTimersByTime(FAILURE_STREAK_GRACE_MS)
    expect(getConnectivitySnapshot()).toBe(`offline`)

    reportTransportSuccess()
    expect(getConnectivitySnapshot()).toBe(`ok`)
    unsubscribe()
  })

  it(`counts a 5xx response as a failure and anything below it as a success`, () => {
    const unsubscribe = subscribeConnectivity(() => {})
    // A proxy answering 502 while the app or Electric is down must alarm,
    // even though the round trip itself completed.
    reportTransportResponse(502, `shape request`)
    vi.advanceTimersByTime(FAILURE_STREAK_GRACE_MS)
    expect(getConnectivitySnapshot()).toBe(`offline`)

    // A 401 or a 409 still proves the server answered.
    reportTransportResponse(401, `shape request`)
    expect(getConnectivitySnapshot()).toBe(`ok`)
    unsubscribe()
  })

  it(`reports offline immediately when the OS says the network is down`, () => {
    const listener = vi.fn()
    const unsubscribe = subscribeConnectivity(listener)
    expect(getConnectivitySnapshot()).toBe(`ok`)

    Object.defineProperty(navigator, `onLine`, {
      configurable: true,
      value: false,
    })
    window.dispatchEvent(new Event(`offline`))
    // No grace window: the OS already knows there is no network.
    expect(getConnectivitySnapshot()).toBe(`offline`)
    expect(listener).toHaveBeenCalled()
    unsubscribe()
  })

  it(`detaches its window listeners with the last subscriber`, () => {
    const remove = vi.spyOn(window, `removeEventListener`)
    const a = subscribeConnectivity(() => {})
    const b = subscribeConnectivity(() => {})
    a()
    expect(remove).not.toHaveBeenCalledWith(`offline`, expect.anything())
    b()
    expect(remove).toHaveBeenCalledWith(`offline`, expect.anything())
    remove.mockRestore()
  })
})
