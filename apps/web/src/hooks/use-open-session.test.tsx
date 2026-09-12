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
  issueSessionTarget,
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
      // EXP-856: `run` names the session, so the page steers the run that was
      // clicked and not merely the issue's newest own one.
      search: { from: `issue:web:MET-12`, run: `s1` },
    })
  })

  // EXP-856: a run the caller KNOWS belongs to another issue never lands on
  // this issue's URL — it would silently show a different run.
  it(`sends a run of another issue to the flat session route`, () => {
    mockState.pathname = `/t/acme/boards/web/issues/MET-12`
    run({ id: `s7`, issueId: `i2` }, { originIssueId: `i1` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: SESSION_ROUTE,
      params: { teamSlug: `acme`, sessionId: `s7` },
      search: { from: `issue:web:MET-12` },
    })
  })

  it(`keeps the issue route when the caller's issue id matches`, () => {
    mockState.pathname = `/t/acme/boards/web/issues/MET-12`
    run({ id: `s8`, issueId: `i1` }, { originIssueId: `i1` })
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: ISSUE_SESSION_ROUTE,
      params: {
        teamSlug: `acme`,
        boardSlug: `web`,
        issueIdentifier: `MET-12`,
      },
      search: { from: `issue:web:MET-12`, run: `s8` },
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
      search: { from: `issue:web:MET-12`, run: `s9` },
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
      search: { from: `issue:web:MET-12`, run: `s1` },
    })
  })

  // EXP-856: `run` rides even when there is no origin token to carry.
  it(`names the run with no token at all`, () => {
    expect(
      sessionNavigation(
        `acme`,
        session,
        { kind: `issue`, boardSlug: `web`, identifier: `MET-12` },
        `i1`
      ).search
    ).toEqual({ from: `issue:web:MET-12`, run: `s1` })
  })

  it(`falls back to the flat route on an issue id mismatch`, () => {
    expect(
      sessionNavigation(
        `acme`,
        session,
        { kind: `issue`, boardSlug: `web`, identifier: `MET-12` },
        `i2`
      )
    ).toEqual({
      to: SESSION_ROUTE,
      params: { teamSlug: `acme`, sessionId: `s1` },
      search: { from: `issue:web:MET-12` },
    })
  })
})

// EXP-856: which run the issue-scoped session route steers.
describe(`issueSessionTarget`, () => {
  const row = (id: string, userId: string, startedAt: string) => ({
    id,
    userId,
    startedAt,
  })
  const rows = [
    row(`old`, `me`, `2026-09-01T10:00:00Z`),
    row(`new`, `me`, `2026-09-02T10:00:00Z`),
    row(`theirs`, `them`, `2026-09-03T10:00:00Z`),
  ]

  it(`steers the run the URL names`, () => {
    expect(issueSessionTarget(rows, `old`, `me`)?.id).toBe(`old`)
  })

  // A teammate's run on this issue is still THIS issue's run — the page
  // renders the owner-only stub for it (EXP-312), which is the honest answer.
  it(`names a teammate's run too`, () => {
    expect(issueSessionTarget(rows, `theirs`, `me`)?.id).toBe(`theirs`)
  })

  it(`falls back to the newest own run without a name`, () => {
    expect(issueSessionTarget(rows, undefined, `me`)?.id).toBe(`new`)
  })

  // The rows are the ISSUE's own, so an id that is not among them belongs to
  // another issue: the fallback, never a blank page.
  it(`falls back when the name is not this issue's run`, () => {
    expect(issueSessionTarget(rows, `elsewhere`, `me`)?.id).toBe(`new`)
  })

  it(`is null when the caller has no run here`, () => {
    expect(issueSessionTarget(rows, undefined, `nobody`)).toBeNull()
    expect(issueSessionTarget([], `old`, `me`)).toBeNull()
  })
})
