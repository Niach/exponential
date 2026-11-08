import { afterEach, beforeAll, describe, expect, it, vi } from "vitest"
import {
  readSendDiagnostics,
  resetSendDiagnosticsForTest,
  SEND_DIAGNOSTICS_STORAGE_KEY,
  subscribeSendDiagnostics,
  writeSendDiagnostics,
} from "./preference"

// The runner's jsdom does not always ship a working localStorage (see
// last-visited.test.ts): a Map-backed one.
beforeAll(() => {
  const map = new Map<string, string>()
  Object.defineProperty(window, `localStorage`, {
    configurable: true,
    value: {
      get length() {
        return map.size
      },
      clear: () => map.clear(),
      getItem: (k: string) => map.get(k) ?? null,
      key: (i: number) => [...map.keys()][i] ?? null,
      removeItem: (k: string) => void map.delete(k),
      setItem: (k: string, v: string) => void map.set(k, v),
    } satisfies Storage,
  })
})

afterEach(() => {
  window.localStorage.clear()
  resetSendDiagnosticsForTest()
})

describe(`Send diagnostics (per device)`, () => {
  it(`defaults to on`, () => {
    expect(readSendDiagnostics()).toBe(true)
  })

  it(`persists off locally and notifies`, () => {
    const listener = vi.fn()
    const unsubscribe = subscribeSendDiagnostics(listener)
    writeSendDiagnostics(false)
    expect(window.localStorage.getItem(SEND_DIAGNOSTICS_STORAGE_KEY)).toBe(`off`)
    resetSendDiagnosticsForTest()
    expect(readSendDiagnostics()).toBe(false)
    writeSendDiagnostics(true)
    expect(window.localStorage.getItem(SEND_DIAGNOSTICS_STORAGE_KEY)).toBeNull()
    expect(listener).toHaveBeenCalledTimes(2)
    unsubscribe()
  })
})
