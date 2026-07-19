import { beforeAll, beforeEach, describe, expect, it } from "vitest"
import {
  clearLastVisited,
  readLastVisited,
  rememberLastVisited,
} from "./last-visited"

const STORAGE_KEY = `exp.lastVisited`

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

describe(`last-visited persistence`, () => {
  beforeAll(() => {
    Object.defineProperty(window, `localStorage`, {
      value: memoryStorage(),
      configurable: true,
    })
  })

  beforeEach(() => {
    window.localStorage.clear()
  })

  it(`returns null when nothing is stored`, () => {
    expect(readLastVisited()).toBeNull()
  })

  it(`round-trips a team + board visit`, () => {
    rememberLastVisited(`acme`, `frontend`)
    expect(readLastVisited()).toEqual({
      teamSlug: `acme`,
      boardSlug: `frontend`,
    })
  })

  it(`keeps the stored board on a team-level visit to the same team`, () => {
    rememberLastVisited(`acme`, `frontend`)
    // Detour through Inbox / My Issues — no board in the URL.
    rememberLastVisited(`acme`)
    expect(readLastVisited()).toEqual({
      teamSlug: `acme`,
      boardSlug: `frontend`,
    })
  })

  it(`drops the board when the team changes without a board`, () => {
    rememberLastVisited(`acme`, `frontend`)
    rememberLastVisited(`other`)
    expect(readLastVisited()).toEqual({
      teamSlug: `other`,
      boardSlug: undefined,
    })
  })

  it(`overwrites the board on a board visit in another team`, () => {
    rememberLastVisited(`acme`, `frontend`)
    rememberLastVisited(`other`, `backend`)
    expect(readLastVisited()).toEqual({
      teamSlug: `other`,
      boardSlug: `backend`,
    })
  })

  it(`clearLastVisited removes the entry`, () => {
    rememberLastVisited(`acme`, `frontend`)
    clearLastVisited()
    expect(readLastVisited()).toBeNull()
    expect(window.localStorage.getItem(STORAGE_KEY)).toBeNull()
  })

  it(`treats malformed JSON as absent`, () => {
    window.localStorage.setItem(STORAGE_KEY, `{not json`)
    expect(readLastVisited()).toBeNull()
  })

  it(`treats entries without a team slug as absent`, () => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ boardSlug: `frontend` })
    )
    expect(readLastVisited()).toBeNull()
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ teamSlug: `` })
    )
    expect(readLastVisited()).toBeNull()
  })

  it(`ignores non-string board slugs`, () => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ teamSlug: `acme`, boardSlug: 42 })
    )
    expect(readLastVisited()).toEqual({
      teamSlug: `acme`,
      boardSlug: undefined,
    })
  })
})
