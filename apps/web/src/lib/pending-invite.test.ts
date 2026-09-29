import { beforeAll, beforeEach, describe, expect, it } from "vitest"
import {
  clearPendingInviteFor,
  parsePendingInvite,
  readPendingInvite,
  rememberPendingInvite,
} from "@/lib/pending-invite"

const DAY = 24 * 60 * 60 * 1000

describe(`parsePendingInvite`, () => {
  const now = 1_800_000_000_000
  const raw = (value: unknown) => JSON.stringify(value)

  it(`returns a fresh token`, () => {
    expect(parsePendingInvite(raw({ token: `abc`, savedAt: now - 1000 }), now)).toBe(
      `abc`
    )
  })

  it(`drops one older than a day`, () => {
    expect(
      parsePendingInvite(raw({ token: `abc`, savedAt: now - DAY - 1 }), now)
    ).toBeNull()
  })

  it(`drops malformed payloads`, () => {
    expect(parsePendingInvite(null, now)).toBeNull()
    expect(parsePendingInvite(`not json`, now)).toBeNull()
    expect(parsePendingInvite(raw({ token: ``, savedAt: now }), now)).toBeNull()
    expect(parsePendingInvite(raw({ token: `abc` }), now)).toBeNull()
  })
})

// In-memory Storage, as in last-visited.test.ts: the runner's jsdom does not
// always ship a working localStorage.
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

describe(`pending invite storage`, () => {
  beforeAll(() => {
    Object.defineProperty(window, `localStorage`, {
      value: memoryStorage(),
      configurable: true,
    })
  })
  beforeEach(() => window.localStorage.clear())

  it(`round-trips and clears only the matching token`, () => {
    rememberPendingInvite(`abc`)
    expect(readPendingInvite()).toBe(`abc`)
    clearPendingInviteFor(`other`)
    expect(readPendingInvite()).toBe(`abc`)
    clearPendingInviteFor(`abc`)
    expect(readPendingInvite()).toBeNull()
  })
})
