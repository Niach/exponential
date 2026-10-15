import { renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/collections`, () => ({ deviceCollection: {} }))
vi.mock(`@tanstack/react-db`, () => ({
  useLiveQuery: () => ({
    data: [
      { id: `row-1`, deviceId: `screenshot-demo-desktop`, label: `Alex's MacBook Pro`, kind: `desktop`, icon: null },
    ],
  }),
}))

import { useDeviceGlyphs } from "@/hooks/use-device-glyphs"

// EXP-1318: a thread row's device glyph resolves the root's `device_id`, the
// device's LOGICAL id (`devices.device_id`), with the row id as a fallback.
describe(`useDeviceGlyphs`, () => {
  it(`resolves the logical device id, else the row id, else nothing`, () => {
    const { result } = renderHook(() => useDeviceGlyphs())
    expect(result.current(`screenshot-demo-desktop`)?.name).toBe(`Alex's MacBook Pro`)
    expect(result.current(`row-1`)?.name).toBe(`Alex's MacBook Pro`)
    expect(result.current(`unknown`)).toBeNull()
    expect(result.current(null)).toBeNull()
  })
})
