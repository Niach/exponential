import { beforeEach, describe, expect, it, vi } from "vitest"
import {
  _clearStartThreads,
  rememberStartThread,
  startThreadSubjectKey,
  takeStartThread,
} from "./start-threads"

// EXP-1284 (review T16): the thread a start frame named, kept for the
// device's codingSessions.start when an older daemon drops the field. Every
// start is its own FIFO entry; an echoed startId takes exactly its own.

const ISSUE = { issueId: `issue-1` }
const remember = (startId: string, threadId: string, over: Record<string, unknown> = {}) =>
  rememberStartThread({
    ownerId: `owner`,
    deviceId: `mac-1`,
    subject: ISSUE,
    threadId,
    startId,
    ...over,
  })
const take = (over: Record<string, unknown> = {}) =>
  takeStartThread({ ownerId: `owner`, deviceId: `mac-1`, subject: ISSUE, ...over })

beforeEach(() => {
  _clearStartThreads()
  vi.useRealTimers()
})

describe(`start-threads`, () => {
  it(`keeps every start of one subject, first in first out, never overwriting`, () => {
    remember(`a`, `t-1`)
    remember(`b`, `t-2`)
    expect(take()).toBe(`t-1`)
    expect(take()).toBe(`t-2`)
    expect(take()).toBeNull()
  })

  it(`an echoed startId takes exactly its own entry, else nothing`, () => {
    remember(`a`, `t-1`)
    remember(`b`, `t-2`)
    expect(take({ startId: `b` })).toBe(`t-2`)
    expect(take({ startId: `b` })).toBeNull()
    expect(take({ startId: `zzz` })).toBeNull()
    expect(take({ startId: `a` })).toBe(`t-1`)
  })

  it(`never hands an entry to another subject, device or owner, even with its startId`, () => {
    remember(`a`, `t-1`)
    expect(take({ subject: { issueId: `issue-2` }, startId: `a` })).toBeNull()
    expect(take({ deviceId: `mac-2`, startId: `a` })).toBeNull()
    expect(take({ ownerId: `someone`, startId: `a` })).toBeNull()
    expect(take({ deviceId: null })).toBeNull()
    expect(take({ startId: `a` })).toBe(`t-1`)
  })

  it(`re-remembering the same startId (a deduped repeat) is a no-op`, () => {
    remember(`a`, `t-1`)
    remember(`a`, `t-9`)
    expect(take()).toBe(`t-1`)
    expect(take()).toBeNull()
  })

  it(`expires entries after five minutes`, () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date(`2026-10-12T12:00:00Z`))
    remember(`a`, `t-1`)
    vi.setSystemTime(new Date(`2026-10-12T12:05:01Z`))
    expect(take()).toBeNull()
  })

  it(`a batch is its set of issues, an action its id`, () => {
    expect(startThreadSubjectKey({ issueIds: [`b`, `a`, `b`] })).toBe(
      startThreadSubjectKey({ issueIds: [`a`, `b`] })
    )
    expect(startThreadSubjectKey({ actionId: `builtin:chat` })).toBe(`action:builtin:chat`)
  })
})
