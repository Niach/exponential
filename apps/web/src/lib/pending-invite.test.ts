import { beforeAll, beforeEach, describe, expect, it } from "vitest"
import {
  clearPendingInviteFor,
  loginErrorFromSearch,
  parsePendingInvite,
  readPendingInvite,
  rememberPendingInvite,
  resolveLoginDestination,
} from "@/lib/pending-invite"
import { oauthErrorMessage } from "@/lib/deep-link"

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

describe(`resolveLoginDestination`, () => {
  it(`lets an explicit destination win over a remembered invite`, () => {
    expect(resolveLoginDestination(`/t/acme`, `abc`)).toBe(`/t/acme`)
    expect(resolveLoginDestination(`/api/mobile-oauth-return`, `abc`)).toBe(
      `/api/mobile-oauth-return`
    )
  })

  it(`resumes the remembered invite when the redirect was lost`, () => {
    expect(resolveLoginDestination(undefined, `abc`)).toBe(`/invite/abc`)
    expect(resolveLoginDestination(``, `abc`)).toBe(`/invite/abc`)
    expect(resolveLoginDestination(null, `abc`)).toBe(`/invite/abc`)
  })

  it(`falls back to the root with neither`, () => {
    expect(resolveLoginDestination(undefined, null)).toBe(`/`)
    expect(resolveLoginDestination(``, undefined)).toBe(`/`)
  })
})

describe(`loginErrorFromSearch`, () => {
  it(`keeps the native hop's error reason`, () => {
    expect(loginErrorFromSearch({ error: `access_denied` })).toBe(`access_denied`)
    expect(loginErrorFromSearch({ error: `state_mismatch`, state: `x` })).toBe(
      `state_mismatch`
    )
  })

  it(`folds Better Auth's state-less callback marker into the error`, () => {
    expect(loginErrorFromSearch({ state: `state_not_found` })).toBe(`state_not_found`)
    // Same copy as the state-cookie drop it is a variant of.
    expect(oauthErrorMessage(loginErrorFromSearch({ state: `state_not_found` }))).toBe(
      oauthErrorMessage(`state_mismatch`)
    )
  })

  it(`ignores other or non-string values`, () => {
    expect(loginErrorFromSearch({})).toBeUndefined()
    expect(loginErrorFromSearch({ error: `` })).toBeUndefined()
    expect(loginErrorFromSearch({ error: 42 })).toBeUndefined()
    expect(loginErrorFromSearch({ state: `abc123` })).toBeUndefined()
    expect(loginErrorFromSearch({ state: [`state_not_found`] })).toBeUndefined()
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
