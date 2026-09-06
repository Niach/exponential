import { renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const mockState = vi.hoisted(() => ({
  navigate: vi.fn(),
  params: {} as Record<string, string | undefined>,
}))

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => mockState.navigate,
  useParams: () => mockState.params,
}))

import { useOpenSession } from "@/hooks/use-open-session"

function run(session: {
  id: string
  issueId?: string | null
  actionId?: string | null
  actionName?: string | null
}) {
  const { result } = renderHook(() => useOpenSession())
  result.current({
    id: session.id,
    issueId: session.issueId ?? null,
    actionId: session.actionId ?? null,
    actionName: session.actionName ?? null,
  } as never)
}

describe(`useOpenSession`, () => {
  beforeEach(() => {
    mockState.navigate.mockClear()
    mockState.params = { teamSlug: `acme` }
  })

  it(`opens an issue run on its own session route`, () => {
    run({ id: `s1`, issueId: `i1` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s1` },
    })
  })

  it(`opens an action run on its own session route`, () => {
    run({ id: `s2`, actionId: `a1`, actionName: `Nightly triage` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s2` },
    })
  })

  // EXP-739: chat is ONE conversation per team, so every chat run lands on
  // the same page rather than getting a session route of its own.
  it(`sends a chat run to the team chat page, naming it`, () => {
    run({ id: `s3`, actionName: `Chat` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/chat`,
      params: { teamSlug: `acme` },
      // Without the id a SECOND running chat would be unreachable: the page
      // defaults to the newest one.
      search: { session: `s3` },
    })
  })

  it(`does nothing without a team slug in scope`, () => {
    mockState.params = {}
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    run({ id: `s4`, issueId: `i1` })
    expect(mockState.navigate).not.toHaveBeenCalled()
    warn.mockRestore()
  })
})
