import { renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const mockState = vi.hoisted(() => ({
  navigate: vi.fn(),
  params: {} as Record<string, string | undefined>,
  pathname: `/t/acme/agent`,
  search: {} as Record<string, unknown>,
}))

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => mockState.navigate,
  useParams: () => mockState.params,
  useLocation: ({ select }: { select: (l: { pathname: string }) => unknown }) =>
    select({ pathname: mockState.pathname }),
  useSearch: ({ select }: { select: (s: Record<string, unknown>) => unknown }) =>
    select(mockState.search),
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
    // The Agent page is the default origin: its own list, no `?from=`.
    mockState.pathname = `/t/acme/agent`
    mockState.search = {}
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

  // EXP-818: a chat run is a session like any other — its own route inside
  // the Agent shell (the chat page's `?session=` detour is gone).
  it(`opens a chat run on its own session route too`, () => {
    run({ id: `s3`, actionName: `Chat` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s3` },
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

// EXP-818: the run carries WHERE it was opened from, so Back returns there.
describe(`useOpenSession origin`, () => {
  beforeEach(() => {
    mockState.navigate.mockClear()
    mockState.params = { teamSlug: `acme` }
    mockState.search = {}
  })

  it(`keeps the inbox when a session is opened while the inbox is up`, () => {
    mockState.pathname = `/t/acme/inbox`
    run({ id: `s1`, issueId: `i1` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s1` },
      search: { from: `inbox` },
    })
  })

  it(`returns to the issue a Watch was clicked on`, () => {
    mockState.pathname = `/t/acme/boards/web/issues/MET-12`
    run({ id: `s1`, issueId: `i1` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s1` },
      search: { from: `issue:web:MET-12` },
    })
  })

  it(`carries the origin a detail already holds`, () => {
    // Inbox → issue → Watch: still the inbox.
    mockState.pathname = `/t/acme/boards/web/issues/MET-12`
    mockState.search = { from: `inbox` }
    run({ id: `s1`, issueId: `i1` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s1` },
      search: { from: `inbox` },
    })
  })

  it(`brings the Agent list along from a full page`, () => {
    mockState.pathname = `/t/acme/devices`
    run({ id: `s1`, actionName: `Nightly triage` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s1` },
    })
  })
})
