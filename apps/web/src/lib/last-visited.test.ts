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

  it(`round-trips a workspace + project visit`, () => {
    rememberLastVisited(`acme`, `frontend`)
    expect(readLastVisited()).toEqual({
      workspaceSlug: `acme`,
      projectSlug: `frontend`,
    })
  })

  it(`keeps the stored project on a workspace-level visit to the same workspace`, () => {
    rememberLastVisited(`acme`, `frontend`)
    // Detour through Inbox / My Issues — no project in the URL.
    rememberLastVisited(`acme`)
    expect(readLastVisited()).toEqual({
      workspaceSlug: `acme`,
      projectSlug: `frontend`,
    })
  })

  it(`drops the project when the workspace changes without a project`, () => {
    rememberLastVisited(`acme`, `frontend`)
    rememberLastVisited(`other`)
    expect(readLastVisited()).toEqual({
      workspaceSlug: `other`,
      projectSlug: undefined,
    })
  })

  it(`overwrites the project on a project visit in another workspace`, () => {
    rememberLastVisited(`acme`, `frontend`)
    rememberLastVisited(`other`, `backend`)
    expect(readLastVisited()).toEqual({
      workspaceSlug: `other`,
      projectSlug: `backend`,
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

  it(`treats entries without a workspace slug as absent`, () => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ projectSlug: `frontend` })
    )
    expect(readLastVisited()).toBeNull()
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ workspaceSlug: `` })
    )
    expect(readLastVisited()).toBeNull()
  })

  it(`ignores non-string project slugs`, () => {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ workspaceSlug: `acme`, projectSlug: 42 })
    )
    expect(readLastVisited()).toEqual({
      workspaceSlug: `acme`,
      projectSlug: undefined,
    })
  })
})
