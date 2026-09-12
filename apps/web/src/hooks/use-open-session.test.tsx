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

import {
  sessionNavigation,
  useOpenSession,
  type OpenSessionOptions,
} from "@/hooks/use-open-session"

function run(
  session: {
    id: string
    issueId?: string | null
    actionId?: string | null
    actionName?: string | null
  },
  options?: OpenSessionOptions
) {
  const { result } = renderHook(() => useOpenSession())
  result.current(
    {
      id: session.id,
      issueId: session.issueId ?? null,
      actionId: session.actionId ?? null,
      actionName: session.actionName ?? null,
    } as never,
    options
  )
}

const SESSION_ROUTE = `/t/$teamSlug/sessions/$sessionId`
const ISSUE_SESSION_ROUTE = `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier/session`

describe(`useOpenSession`, () => {
  beforeEach(() => {
    mockState.navigate.mockClear()
    mockState.params = { teamSlug: `acme` }
    // The Agent page is the default origin: its own list.
    mockState.pathname = `/t/acme/agent`
    mockState.search = {}
  })

  it(`opens an issue run on its own session route`, () => {
    run({ id: `s1`, issueId: `i1` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: SESSION_ROUTE,
      params: { teamSlug: `acme`, sessionId: `s1` },
      search: { from: `agent` },
    })
  })

  it(`opens an action run on its own session route`, () => {
    run({ id: `s2`, actionId: `a1`, actionName: `Nightly triage` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: SESSION_ROUTE,
      params: { teamSlug: `acme`, sessionId: `s2` },
      search: { from: `agent` },
    })
  })

  // EXP-818: a chat run is a session like any other — its own route.
  it(`opens a chat run on its own session route too`, () => {
    run({ id: `s3`, actionName: `Chat` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: SESSION_ROUTE,
      params: { teamSlug: `acme`, sessionId: `s3` },
      search: { from: `agent` },
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

// EXP-818/EXP-851: the run carries WHERE it was opened from, so the sidebar
// keeps that list and Back returns there.
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
      to: SESSION_ROUTE,
      params: { teamSlug: `acme`, sessionId: `s1` },
      search: { from: `inbox` },
    })
  })

  // EXP-851: the issue's own run lives on the issue's URL.
  it(`opens an issue's run on the issue's session route`, () => {
    mockState.pathname = `/t/acme/boards/web/issues/MET-12`
    run({ id: `s1`, issueId: `i1` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: ISSUE_SESSION_ROUTE,
      params: {
        teamSlug: `acme`,
        boardSlug: `web`,
        issueIdentifier: `MET-12`,
      },
      search: { from: `issue:web:MET-12` },
    })
  })

  // …and the launcher hop keeps it: the composer carries `?from=issue:…`, so
  // the run it starts still lands on the issue.
  it(`keeps the issue through the Agent page`, () => {
    mockState.pathname = `/t/acme/agent`
    mockState.search = { from: `issue:web:MET-12` }
    run({ id: `s9`, issueId: `i1` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: ISSUE_SESSION_ROUTE,
      params: {
        teamSlug: `acme`,
        boardSlug: `web`,
        issueIdentifier: `MET-12`,
      },
      search: { from: `issue:web:MET-12` },
    })
  })

  // An action/batch/chat run started from an issue is NOT that issue's run.
  it(`sends an issue-less run to the flat session route`, () => {
    mockState.pathname = `/t/acme/boards/web/issues/MET-12`
    run({ id: `s2`, actionName: `Chat` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: SESSION_ROUTE,
      params: { teamSlug: `acme`, sessionId: `s2` },
      search: { from: `issue:web:MET-12` },
    })
  })

  it(`brings no list along from a full page`, () => {
    mockState.pathname = `/t/acme/devices`
    run({ id: `s1`, actionName: `Nightly triage` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: SESSION_ROUTE,
      params: { teamSlug: `acme`, sessionId: `s1` },
    })
  })

  // EXP-851: a list hands its own token; a context-free row hands `null`, and
  // the URL it was clicked on stops mattering.
  it(`takes an explicit origin over the location`, () => {
    mockState.pathname = `/t/acme/inbox`
    run({ id: `s1`, issueId: `i1` }, { origin: null })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: SESSION_ROUTE,
      params: { teamSlug: `acme`, sessionId: `s1` },
    })
    mockState.navigate.mockClear()
    run({ id: `s1`, issueId: `i1` }, { origin: { kind: `agent` } })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: SESSION_ROUTE,
      params: { teamSlug: `acme`, sessionId: `s1` },
      search: { from: `agent` },
    })
  })
})

describe(`sessionNavigation`, () => {
  const session = {
    id: `s1`,
    issueId: `i1`,
    actionId: null,
    actionName: null,
  }

  it(`is pure: origin in, destination out`, () => {
    expect(sessionNavigation(`acme`, session, null)).toEqual({
      to: SESSION_ROUTE,
      params: { teamSlug: `acme`, sessionId: `s1` },
    })
    expect(
      sessionNavigation(`acme`, session, { kind: `support` })
    ).toEqual({
      to: SESSION_ROUTE,
      params: { teamSlug: `acme`, sessionId: `s1` },
      search: { from: `support` },
    })
    expect(
      sessionNavigation(`acme`, session, {
        kind: `issue`,
        boardSlug: `web`,
        identifier: `MET-12`,
      })
    ).toEqual({
      to: ISSUE_SESSION_ROUTE,
      params: {
        teamSlug: `acme`,
        boardSlug: `web`,
        issueIdentifier: `MET-12`,
      },
      search: { from: `issue:web:MET-12` },
    })
  })
})
