import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest"
import { act, cleanup, renderHook } from "@testing-library/react"
import {
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
  SIDEBAR_WIDTHS_STORAGE_KEY,
  clampSidebarWidth,
  parseSidebarWidths,
  resetSidebarWidth,
  resetSidebarWidthsStoreForTests,
  setSidebarWidth,
  sidebarDefaultWidth,
  sidebarWidthBounds,
  sidebarUnitScale,
  sidebarWidthCss,
  useSidebarWidth,
  useSidebarWidthBounds,
} from "@/lib/sidebar-widths"

describe(`sidebar width tokens`, () => {
  it(`gives every panel its default`, () => {
    expect(sidebarDefaultWidth(`main`)).toBe(272)
    expect(sidebarDefaultWidth(`settings`)).toBe(272)
  })

  it(`renders design units as rem, so the default stays 17rem`, () => {
    expect(sidebarWidthCss(272)).toBe(`17rem`)
  })
})

describe(`clampSidebarWidth`, () => {
  it(`clamps to the shared min and max`, () => {
    expect(clampSidebarWidth(100)).toBe(SIDEBAR_MIN_WIDTH)
    expect(clampSidebarWidth(9999)).toBe(SIDEBAR_MAX_WIDTH)
    expect(clampSidebarWidth(400.4)).toBe(400)
  })

  it(`keeps the whole column at or under half the viewport`, () => {
    expect(clampSidebarWidth(540, { viewportWidth: 1000 })).toBe(500)
    // A wide window leaves the shared max in charge.
    expect(clampSidebarWidth(9999, { viewportWidth: 4000 })).toBe(
      SIDEBAR_MAX_WIDTH
    )
  })

  it(`never goes below the minimum on a narrow window`, () => {
    expect(clampSidebarWidth(400, { viewportWidth: 500 })).toBe(SIDEBAR_MIN_WIDTH)
    expect(sidebarWidthBounds({ viewportWidth: 500 })).toEqual({
      min: SIDEBAR_MIN_WIDTH,
      max: SIDEBAR_MIN_WIDTH,
    })
  })
})

describe(`parseSidebarWidths`, () => {
  it(`reads known finite numbers`, () => {
    expect(parseSidebarWidths(`{"settings":300}`)).toEqual({ settings: 300 })
  })

  it(`never reads a width for the fixed main menu`, () => {
    expect(parseSidebarWidths(`{"main":300}`)).toEqual({})
  })

  it(`drops garbage, non-finite values and unknown keys`, () => {
    expect(parseSidebarWidths(null)).toEqual({})
    expect(parseSidebarWidths(`not json`)).toEqual({})
    expect(parseSidebarWidths(`[1,2]`)).toEqual({})
    expect(parseSidebarWidths(`42`)).toEqual({})
    expect(
      parseSidebarWidths(
        `{"main":"300","list":null,"review":1e999,"files":320,"settings":290}`
      )
    ).toEqual({ settings: 290 })
  })
})

// Minimal in-memory Storage (as in last-visited.test.ts): the runner's jsdom
// does not always ship a working localStorage.
function memoryStorage(): Storage {
  const map = new Map<string, string>()
  return {
    get length() {
      return map.size
    },
    clear: () => map.clear(),
    getItem: (key: string) => map.get(key) ?? null,
    key: (index: number) => [...map.keys()][index] ?? null,
    removeItem: (key: string) => void map.delete(key),
    setItem: (key: string, value: string) => void map.set(key, value),
  }
}

describe(`the width store`, () => {
  beforeAll(() => {
    Object.defineProperty(window, `localStorage`, {
      value: memoryStorage(),
      configurable: true,
    })
  })
  beforeEach(() => {
    window.localStorage.clear()
    resetSidebarWidthsStoreForTests()
  })
  afterEach(() => {
    cleanup()
    window.localStorage.clear()
    resetSidebarWidthsStoreForTests()
  })

  it(`returns the stored width, clamped, and the default otherwise`, () => {
    window.localStorage.setItem(
      SIDEBAR_WIDTHS_STORAGE_KEY,
      JSON.stringify({ settings: 100, main: 400 })
    )
    const settings = renderHook(() => useSidebarWidth(`settings`))
    expect(settings.result.current).toBe(SIDEBAR_MIN_WIDTH)
    const main = renderHook(() => useSidebarWidth(`main`))
    expect(main.result.current).toBe(272)
  })

  it(`updates in memory at once and persists only when asked`, () => {
    const { result } = renderHook(() => useSidebarWidth(`settings`))
    act(() => setSidebarWidth(`settings`, 380, { persist: false }))
    expect(result.current).toBe(380)
    expect(window.localStorage.getItem(SIDEBAR_WIDTHS_STORAGE_KEY)).toBeNull()
    act(() => setSidebarWidth(`settings`, 390))
    expect(result.current).toBe(390)
    expect(
      JSON.parse(window.localStorage.getItem(SIDEBAR_WIDTHS_STORAGE_KEY)!)
    ).toEqual({ settings: 390 })
  })

  it(`reset drops the key and falls back to the default`, () => {
    const { result } = renderHook(() => useSidebarWidth(`settings`))
    act(() => setSidebarWidth(`settings`, 320))
    act(() => resetSidebarWidth(`settings`))
    expect(result.current).toBe(272)
    expect(
      JSON.parse(window.localStorage.getItem(SIDEBAR_WIDTHS_STORAGE_KEY)!)
    ).toEqual({})
  })

  it(`follows the viewport: half the window bounds the column`, () => {
    const scale = sidebarUnitScale()
    window.innerWidth = 600 * scale
    const width = renderHook(() => useSidebarWidth(`settings`))
    const bounds = renderHook(() => useSidebarWidthBounds(`settings`))
    expect(width.result.current).toBe(272)
    expect(bounds.result.current.max).toBe(300)
    act(() => {
      window.innerWidth = 2000 * scale
      window.dispatchEvent(new Event(`resize`))
    })
    expect(width.result.current).toBe(272)
    expect(bounds.result.current.max).toBe(SIDEBAR_MAX_WIDTH)
  })
})
