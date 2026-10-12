import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { CodingSession } from "@/db/schema"

// EXP-1284 (review T11): every start lands on its THREAD (the Conversation
// child); a thread named up front wins while the item syncs; with no thread
// within the wait, the session's own page opens.

const mocks = vi.hoisted(() => ({
  navigate: vi.fn(),
  openSession: vi.fn(),
  threadIdOfRun: vi.fn((_id: string): string | null => null),
  waitForThreadOfRun: vi.fn(async (_id: string): Promise<string | null> => null),
}))

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => mocks.navigate,
  useParams: () => ({ teamSlug: `acme` }),
}))
vi.mock(`@/hooks/use-open-session`, () => ({ useOpenSession: () => mocks.openSession }))
vi.mock(`@/hooks/use-threads`, () => ({
  threadIdOfRun: mocks.threadIdOfRun,
  waitForThreadOfRun: mocks.waitForThreadOfRun,
}))

import { useOpenStartedSession } from "@/hooks/use-open-started-session"

const session = { id: `r1` } as CodingSession

beforeEach(() => {
  mocks.navigate.mockClear()
  mocks.openSession.mockClear()
  mocks.threadIdOfRun.mockReset()
  mocks.threadIdOfRun.mockReturnValue(null)
  mocks.waitForThreadOfRun.mockReset()
  mocks.waitForThreadOfRun.mockResolvedValue(null)
})

describe(`useOpenStartedSession`, () => {
  it(`lands on the session's thread, keeping the list origin`, () => {
    mocks.threadIdOfRun.mockReturnValue(`th1`)
    const { result } = renderHook(() => useOpenStartedSession())
    result.current(session, { origin: { kind: `agent`, tab: `recent` } })
    expect(mocks.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug: `acme`, threadId: `th1` },
      search: { from: `agent:recent` },
    })
    expect(mocks.openSession).not.toHaveBeenCalled()
  })

  it(`uses the thread the start named while its item syncs`, () => {
    const { result } = renderHook(() => useOpenStartedSession())
    result.current(session, { knownThreadId: `th2` })
    expect(mocks.navigate.mock.calls[0]![0].params.threadId).toBe(`th2`)
  })

  it(`waits for the item, then falls back to the session's page`, async () => {
    const { result } = renderHook(() => useOpenStartedSession())
    await act(async () => {
      result.current(session)
      await Promise.resolve()
    })
    expect(mocks.waitForThreadOfRun).toHaveBeenCalledWith(`r1`)
    expect(mocks.openSession).toHaveBeenCalledWith(session, undefined)
    expect(mocks.navigate).not.toHaveBeenCalled()
  })
})
