import { beforeAll, beforeEach, describe, expect, it } from "vitest"
import {
  clearAgentUsageWindow,
  readAgentUsageWindow,
  writeAgentUsageWindow,
} from "./agent-usage-prefs"

// Minimal in-memory Storage — the test runner's jsdom does not always ship a
// working localStorage, and the helper only needs the Storage contract.
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

describe(`agent usage window prefs`, () => {
  beforeAll(() => {
    Object.defineProperty(window, `localStorage`, {
      value: memoryStorage(),
      configurable: true,
    })
  })

  beforeEach(() => {
    window.localStorage.clear()
  })

  it(`returns null when nothing is pinned`, () => {
    expect(readAgentUsageWindow(`claude`)).toBeNull()
  })

  it(`round-trips a pin under the per-agent key`, () => {
    writeAgentUsageWindow(`claude`, `model:fable`)
    expect(readAgentUsageWindow(`claude`)).toBe(`model:fable`)
    expect(window.localStorage.getItem(`exp.agentUsageWindow.claude`)).toBe(
      `model:fable`
    )
  })

  it(`keys per agent`, () => {
    writeAgentUsageWindow(`claude`, `session`)
    writeAgentUsageWindow(`codex`, `weekly`)
    expect(readAgentUsageWindow(`claude`)).toBe(`session`)
    expect(readAgentUsageWindow(`codex`)).toBe(`weekly`)
  })

  it(`an empty key clears the pin, like clearAgentUsageWindow`, () => {
    writeAgentUsageWindow(`claude`, `session`)
    writeAgentUsageWindow(`claude`, ``)
    expect(readAgentUsageWindow(`claude`)).toBeNull()

    writeAgentUsageWindow(`codex`, `weekly`)
    clearAgentUsageWindow(`codex`)
    expect(readAgentUsageWindow(`codex`)).toBeNull()
  })

  it(`degrades to no pin when storage throws`, () => {
    Object.defineProperty(window, `localStorage`, {
      get() {
        throw new Error(`blocked`)
      },
      configurable: true,
    })
    expect(readAgentUsageWindow(`claude`)).toBeNull()
    expect(() => writeAgentUsageWindow(`claude`, `session`)).not.toThrow()
    expect(() => clearAgentUsageWindow(`claude`)).not.toThrow()
    Object.defineProperty(window, `localStorage`, {
      value: memoryStorage(),
      configurable: true,
    })
  })
})
