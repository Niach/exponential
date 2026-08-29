import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { OfflineBanner } from "@/components/offline-banner"
import {
  FAILURE_STREAK_GRACE_MS,
  reportTransportFailure,
  reportTransportSuccess,
  resetConnectivityForTests,
} from "@/lib/connectivity"

const BANNER = `Can't reach the server, showing cached data`

describe(`OfflineBanner`, () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    resetConnectivityForTests()
  })

  afterEach(() => {
    cleanup()
    resetConnectivityForTests()
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  it(`renders nothing while the server is reachable`, () => {
    render(<OfflineBanner />)
    expect(screen.queryByRole(`status`)).toBeNull()
  })

  it(`appears once the failure streak outlives the grace window`, () => {
    render(<OfflineBanner />)
    act(() => {
      reportTransportFailure(new Error(`Failed to fetch`))
    })
    // The grace window is what keeps a wake-up burst from flashing the banner.
    expect(screen.queryByRole(`status`)).toBeNull()

    act(() => {
      vi.advanceTimersByTime(FAILURE_STREAK_GRACE_MS)
    })
    expect(screen.getByRole(`status`).textContent).toContain(BANNER)
  })

  it(`clears again on the next successful round trip`, () => {
    render(<OfflineBanner />)
    act(() => {
      reportTransportFailure(new Error(`Failed to fetch`))
      vi.advanceTimersByTime(FAILURE_STREAK_GRACE_MS)
    })
    expect(screen.getByRole(`status`)).toBeTruthy()

    act(() => {
      reportTransportSuccess()
    })
    expect(screen.queryByRole(`status`)).toBeNull()
  })

  it(`probes the server behind Retry and dismisses itself when it answers`, async () => {
    const fetchMock = vi
      .spyOn(globalThis, `fetch`)
      .mockResolvedValue(new Response(`{"ok":true}`, { status: 200 }))
    render(<OfflineBanner />)
    act(() => {
      reportTransportFailure(new Error(`Failed to fetch`))
      vi.advanceTimersByTime(FAILURE_STREAK_GRACE_MS)
    })

    await act(async () => {
      fireEvent.click(screen.getByRole(`button`, { name: `Retry` }))
    })

    expect(fetchMock).toHaveBeenCalledWith(`/api/health`, { cache: `no-store` })
    expect(screen.queryByRole(`status`)).toBeNull()
  })

  it(`stays up when the probe fails too`, async () => {
    vi.spyOn(globalThis, `fetch`).mockRejectedValue(new TypeError(`Failed to fetch`))
    render(<OfflineBanner />)
    act(() => {
      reportTransportFailure(new Error(`Failed to fetch`))
      vi.advanceTimersByTime(FAILURE_STREAK_GRACE_MS)
    })

    await act(async () => {
      fireEvent.click(screen.getByRole(`button`, { name: `Retry` }))
    })

    expect(screen.getByRole(`status`).textContent).toContain(BANNER)
  })
})
