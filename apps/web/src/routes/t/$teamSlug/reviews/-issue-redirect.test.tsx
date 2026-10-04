// EXP-1154: the retired Reviews detail URL replaces itself with the issue's
// Changes face, or the Reviews list for an unknown issue. Lives under a `-`
// prefix so the route generator ignores it.
import { render } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  navigate: vi.fn(),
  boards: { current: { boards: [] as unknown[], boardsReady: false } },
  issues: { current: { data: [] as unknown[], isReady: false } },
}))

vi.mock(`@tanstack/react-router`, () => ({
  createFileRoute: () => (options: Record<string, unknown>) => ({
    ...options,
    useParams: () => ({ teamSlug: `acme`, issueIdentifier: `EXP-7` }),
  }),
  useNavigate: () => mocks.navigate,
}))
vi.mock(`@tanstack/react-db`, () => ({
  and: vi.fn(),
  eq: vi.fn(),
  inArray: vi.fn(),
  useLiveQuery: () => mocks.issues.current,
}))
vi.mock(`@/lib/collections`, () => ({ issueCollection: {} }))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBySlug: () => ({ id: `t1`, slug: `acme` }),
  useTeamBoardsWithReady: () => mocks.boards.current,
}))

import { Route } from "./$issueIdentifier"

const Page = (Route as unknown as { component: () => null }).component

beforeEach(() => {
  mocks.navigate.mockClear()
  mocks.boards.current = { boards: [], boardsReady: false }
  mocks.issues.current = { data: [], isReady: false }
})

describe(`/t/$teamSlug/reviews/$issueIdentifier`, () => {
  it(`waits while the boards or the issue are still syncing`, () => {
    render(<Page />)
    mocks.boards.current = {
      boards: [{ id: `b1`, slug: `core` }],
      boardsReady: true,
    }
    render(<Page />)
    expect(mocks.navigate).not.toHaveBeenCalled()
  })

  it(`replaces itself with the issue's Changes face`, () => {
    mocks.boards.current = {
      boards: [{ id: `b1`, slug: `core` }],
      boardsReady: true,
    }
    mocks.issues.current = {
      data: [{ id: `i1`, boardId: `b1`, identifier: `EXP-7` }],
      isReady: true,
    }
    render(<Page />)
    expect(mocks.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: { teamSlug: `acme`, boardSlug: `core`, issueIdentifier: `EXP-7` },
      search: { from: `reviews`, view: `diff` },
      replace: true,
    })
  })

  it(`falls back to the Reviews list for an unknown issue`, () => {
    mocks.boards.current = {
      boards: [{ id: `b1`, slug: `core` }],
      boardsReady: true,
    }
    mocks.issues.current = { data: [], isReady: true }
    render(<Page />)
    expect(mocks.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/reviews`,
      params: { teamSlug: `acme` },
      replace: true,
    })
  })
})
