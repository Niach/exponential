// EXP-1154: a teammate's run. The live view stays the owner's (EXP-312), but
// `?view=results` (where a run without an issue links its PR body footer)
// shows the synced report read-only. Lives under a `-` prefix so the route
// generator ignores it.
import type { ComponentType } from "react"
import { render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  search: { current: {} as { view?: `diff` | `results` } },
  session: { current: null as Record<string, unknown> | null },
}))

vi.mock(`@tanstack/react-router`, () => ({
  createFileRoute: () => (options: Record<string, unknown>) => ({
    ...options,
    useParams: () => ({ teamSlug: `acme`, sessionId: `s1` }),
    useSearch: () => mocks.search.current,
  }),
  Link: ({ children }: { children: unknown }) => children,
  redirect: vi.fn(),
  useNavigate: () => vi.fn(),
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBySlug: () => ({ id: `t1`, slug: `acme` }),
  useTeamUsers: () => ({ users: [] }),
}))
vi.mock(`@/hooks/use-session`, () => ({
  useSession: () => ({ data: { user: { id: `viewer` } } }),
}))
vi.mock(`@/hooks/use-agents-data`, () => ({
  rowPrState: () => null,
  useIssueRuns: () => ({ runs: [] }),
  useRunChain: () => ({ runs: [] }),
  useSessionRow: () => ({
    row: { session: mocks.session.current, issue: null, board: null, batchIssues: [] },
    session: mocks.session.current,
    isReady: true,
  }),
}))
vi.mock(`@/lib/session-identity`, () => ({
  sessionIdentity: () => ({ identifier: null, subject: `Chat run` }),
}))
vi.mock(`@/lib/page-title`, () => ({
  pageTitle: (...parts: string[]) => parts.join(` `),
  usePageTitle: () => {},
}))
vi.mock(`@/components/agent-session`, () => ({
  AgentSessionView: () => <div data-testid="agent-session-view" />,
  renderResultText: (text: string) => <div data-testid="report-markdown">{text}</div>,
}))
vi.mock(`@/components/issue-coding-rows`, () => ({
  SessionStatusBadge: () => <span data-testid="status-badge" />,
}))
vi.mock(`@/components/team/mobile-detail-header`, () => ({
  MobileDetailHeader: ({ title }: { title: string }) => <header>{title}</header>,
  MOBILE_DETAIL_SCREEN_CLASS: ``,
}))
for (const path of [
  `@/components/issue-actions-menu`,
  `@/components/issue-coding-action`,
  `@/components/issue-mobile-header`,
  `@/components/issue-properties-tray`,
  `@/components/pin-toggle-button`,
  `@/components/pr-graph-badge`,
  `@/hooks/use-open-composer`,
  `@/hooks/use-open-session`,
  `@/hooks/use-review-files`,
  `@/hooks/use-issue-property-handlers`,
  `@/hooks/use-team-permissions`,
  `@/lib/collections`,
]) {
  vi.doMock(path, () => ({}))
}

const { Route } = await import(`@/routes/t/$teamSlug/sessions/$sessionId`)
const Page = (Route as unknown as { component: ComponentType }).component

const report = [
  { topic: `Summary`, label: null, attachmentId: null, width: null, height: null, text: `Shipped the chat fix.` },
]

beforeEach(() => {
  mocks.search.current = {}
  mocks.session.current = {
    id: `s1`,
    userId: `owner`,
    status: `ended`,
    results: report,
    prUrl: null,
  }
})

describe(`a teammate's run`, () => {
  it(`shows the synced report read-only on ?view=results`, () => {
    mocks.search.current = { view: `results` }
    render(<Page />)
    expect(screen.getByTestId(`issue-results-face`)).toBeTruthy()
    expect(screen.getByTestId(`report-markdown`).textContent).toBe(`Shipped the chat fix.`)
    expect(screen.queryByText(`Only the owner can steer this session.`)).toBeNull()
    expect(screen.queryByTestId(`agent-session-view`)).toBeNull()
  })

  it(`says so when the run has no report yet`, () => {
    mocks.search.current = { view: `results` }
    mocks.session.current = { ...mocks.session.current!, results: null }
    render(<Page />)
    expect(screen.getByText(`This run has no report yet.`)).toBeTruthy()
    expect(screen.queryByTestId(`agent-session-view`)).toBeNull()
  })

  it(`keeps the owner-only stub on every other view`, () => {
    for (const search of [{}, { view: `diff` as const }]) {
      mocks.search.current = search
      const { unmount } = render(<Page />)
      expect(screen.getByText(`Only the owner can steer this session.`)).toBeTruthy()
      expect(screen.queryByTestId(`issue-results-face`)).toBeNull()
      unmount()
    }
  })
})
