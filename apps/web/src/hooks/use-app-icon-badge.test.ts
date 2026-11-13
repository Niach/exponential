import { describe, expect, it, vi } from "vitest"
import { setAppIconBadge } from "./use-app-icon-badge"

describe(`setAppIconBadge (#FEED-232 appIconBadge)`, () => {
  it(`sets the count, clears at 0, and skips without the Badging API`, () => {
    const nav = {
      setAppBadge: vi.fn(() => Promise.resolve()),
      clearAppBadge: vi.fn(() => Promise.resolve()),
    } as unknown as Navigator
    setAppIconBadge(3, nav)
    expect(nav.setAppBadge).toHaveBeenCalledWith(3)
    setAppIconBadge(0, nav)
    expect(nav.clearAppBadge).toHaveBeenCalled()
    expect(() => setAppIconBadge(2, {} as Navigator)).not.toThrow()
  })
})
