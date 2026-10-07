import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest"
import { act, cleanup, renderHook } from "@testing-library/react"
import {
  parseShowWork,
  resetShowWorkStoreForTests,
  setShowWork,
  showWorkStorageKey,
  useShowWork,
} from "@/lib/show-work-pref"
import { SHOW_WORK_DEFAULT } from "@/lib/work-faces"

// Minimal in-memory Storage — the test runner's jsdom does not always ship a
// working localStorage (same shim as last-visited.test.ts).
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

describe(`Show work preference (EXP-1175)`, () => {
  beforeAll(() => {
    Object.defineProperty(window, `localStorage`, {
      value: memoryStorage(),
      configurable: true,
    })
  })

  beforeEach(() => {
    window.localStorage.clear()
    resetShowWorkStoreForTests()
  })
  afterEach(() => cleanup())

  it(`keys the preference per user`, () => {
    expect(showWorkStorageKey(`u1`)).toBe(`exp.showWork:u1`)
  })

  it(`reads garbage as the default`, () => {
    expect(parseShowWork(null)).toBe(SHOW_WORK_DEFAULT)
    expect(parseShowWork(`yes`)).toBe(SHOW_WORK_DEFAULT)
    expect(parseShowWork(`{"a":1}`)).toBe(SHOW_WORK_DEFAULT)
    expect(parseShowWork(`true`)).toBe(true)
    expect(parseShowWork(`false`)).toBe(false)
  })

  it(`starts at the default with nothing stored`, () => {
    const { result } = renderHook(() => useShowWork(`u1`))
    expect(result.current).toBe(SHOW_WORK_DEFAULT)
  })

  it(`reads a stored value`, () => {
    window.localStorage.setItem(`exp.showWork:u1`, `true`)
    const { result } = renderHook(() => useShowWork(`u1`))
    expect(result.current).toBe(true)
  })

  it(`persists a toggle and re-renders subscribers, per user`, () => {
    const a = renderHook(() => useShowWork(`u1`))
    const b = renderHook(() => useShowWork(`u2`))
    act(() => setShowWork(`u1`, true))
    expect(a.result.current).toBe(true)
    expect(b.result.current).toBe(SHOW_WORK_DEFAULT)
    expect(window.localStorage.getItem(`exp.showWork:u1`)).toBe(`true`)
    resetShowWorkStoreForTests()
    const c = renderHook(() => useShowWork(`u1`))
    expect(c.result.current).toBe(true)
    act(() => setShowWork(`u1`, false))
    expect(c.result.current).toBe(false)
  })
})
