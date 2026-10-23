// EXP-1293 / EXP-1316: `/sessions/$id` — a thread's session REDIRECTS into
// its thread (`?tab=session&session=<id>`, `from` kept); a session under an
// agent's conversation (NEW-2: a root with `agentId`) keeps its own screen: back to the agent, the session's name,
// Stop while live, the feed and the reply box. Lives under a `-` prefix so
// the route generator ignores it.
import type { ComponentType, ReactNode } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  row: { current: null as Record<string, unknown> | null },
  root: { current: null as Record<string, unknown> | null },
  isReady: { current: true },
  search: { current: {} as Record<string, unknown> },
  stop: vi.fn(),
}))

vi.mock(`@tanstack/react-router`, () => ({
  createFileRoute: () => (options: Record<string, unknown>) => ({
    ...options,
    useParams: () => ({ teamSlug: `acme`, sessionId: `s2` }),
    useSearch: () => mocks.search.current,
  }),
  Navigate: ({
    to,
    params,
    search,
  }: {
    to: string
    params: Record<string, string>
    search?: Record<string, string>
  }) => (
    <div
      data-testid="navigate"
      data-to={to}
      data-thread={params.threadId}
      data-tab={search?.tab}
      data-session={search?.session}
      data-from={search?.from}
    />
  ),
  Link: ({
    children,
    params,
    ...rest
  }: {
    children: ReactNode
    params?: Record<string, string>
    "data-testid"?: string
  }) => (
    <a data-testid={rest[`data-testid`]} data-thread={params?.threadId}>
      {children}
    </a>
  ),
  redirect: vi.fn(),
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBySlug: () => ({ id: `t1`, slug: `acme` }),
  useTeamUsers: () => ({ users: [] }),
}))
vi.mock(`@/hooks/use-session-tree`, () => ({
  useSessionRow: (id: string | null) =>
    id === `s1`
      ? { row: mocks.root.current, isReady: true }
      : id
        ? { row: mocks.row.current, isReady: mocks.isReady.current }
        : { row: null, isReady: false },
  useTreeSessions: () => ({ rows: [], tree: [], isReady: true }),
  useSessionCard: (row: { title: string | null } | null) =>
    row ? { name: row.title ?? `Untitled`, icon: `coding-running` } : null,
  useSessionThread: () => ({ view: { title: `Reduce cold start below 800 ms` } }),
  useSessionMessages: () => [],
  useConversationSessions: () => ({ conversations: new Map(), isReady: true }),
}))
vi.mock(`@/hooks/use-agent-row`, () => ({
  useAgentRow: (id: string | null) => ({
    agent: id === `agent-1` ? { id, name: `Triage` } : null,
    isReady: true,
  }),
}))
vi.mock(`@/lib/page-title`, () => ({
  pageTitle: (...parts: string[]) => parts.join(` `),
  usePageTitle: () => {},
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { sessions: { stop: { mutate: mocks.stop } } },
}))
vi.mock(`@/components/agent-session`, () => ({
  AgentSessionView: ({ header, composer }: { header: ReactNode; composer: ReactNode }) => (
    <div data-testid="agent-session-view">
      {header}
      {composer}
    </div>
  ),
}))
vi.mock(`@/components/session-composer`, () => ({
  SessionComposer: ({ placeholder, sessionId }: { placeholder: string; sessionId: string }) => (
    <div data-testid="session-composer" data-session={sessionId}>
      {placeholder}
    </div>
  ),
}))

const { Route } = await import(`@/routes/t/$teamSlug/sessions/$sessionId`)
const Page = (Route as unknown as { component: ComponentType }).component

const row = (over: Record<string, unknown> = {}) => ({
  id: `s2`,
  teamId: `t1`,
  rootId: `s1`,
  parentId: `s1`,
  userId: `u1`,
  status: `running`,
  busy: true,
  needsInput: false,
  title: `Profile the launch path`,
  ...over,
})

beforeEach(() => {
  mocks.row.current = row()
  // NEW-2: an agent's conversation root = a root with `agentId`.
  mocks.root.current = { id: `s1`, agentId: `agent-1`, rootId: `s1`, teamId: `t1` }
  mocks.isReady.current = true
  mocks.search.current = {}
  mocks.stop.mockReset()
  mocks.stop.mockResolvedValue({})
})

describe(`the session page`, () => {
  it(`redirects a thread's session into its thread, keeping from`, () => {
    mocks.root.current = { id: `s1`, agentId: null, rootId: `s1`, teamId: `t1` }
    mocks.search.current = { from: `inbox` }
    render(<Page />)
    const target = screen.getByTestId(`navigate`)
    expect(target.getAttribute(`data-to`)).toBe(`/t/$teamSlug/threads/$threadId`)
    expect(target.getAttribute(`data-thread`)).toBe(`s1`)
    expect(target.getAttribute(`data-tab`)).toBe(`session`)
    expect(target.getAttribute(`data-session`)).toBe(`s2`)
    expect(target.getAttribute(`data-from`)).toBe(`inbox`)
    expect(screen.queryByTestId(`agent-session-view`)).toBeNull()
  })

  it(`keeps an agent's session on its own screen, back to the agent`, () => {
    render(<Page />)
    const back = screen.getByTestId(`session-back`)
    expect(back.textContent).toBe(`Triage`)
    expect(screen.getByTestId(`session-title`).textContent).toBe(`Profile the launch path`)
    const composer = screen.getByTestId(`session-composer`)
    expect(composer.getAttribute(`data-session`)).toBe(`s2`)
    expect(composer.textContent).toBe(`Reply…`)
  })

  it(`offers Stop while live and stops through sessions.stop`, () => {
    render(<Page />)
    fireEvent.click(screen.getByTestId(`session-stop`))
    expect(mocks.stop.mock.calls[0]?.[0]).toEqual({ sessionId: `s2` })
  })

  it(`has no Stop once the session ended`, () => {
    mocks.row.current = row({ status: `ended`, busy: false })
    render(<Page />)
    expect(screen.queryByTestId(`session-stop`)).toBeNull()
    expect(screen.getByTestId(`agent-session-view`)).toBeTruthy()
  })

  it(`says so when the session is not there`, () => {
    mocks.row.current = null
    render(<Page />)
    expect(screen.queryByTestId(`agent-session-view`)).toBeNull()
    expect(screen.getByText(`Session not found`)).toBeTruthy()
  })

  // EXP-1291: the PR body's footer links `sessions/<rootId>?view=guide`.
  it(`opens the thread's Guide for ?view=guide`, () => {
    mocks.search.current = { view: `guide` }
    render(<Page />)
    const target = screen.getByTestId(`navigate`)
    expect(target.getAttribute(`data-to`)).toBe(`/t/$teamSlug/threads/$threadId`)
    expect(target.getAttribute(`data-thread`)).toBe(`s1`)
    expect(target.getAttribute(`data-tab`)).toBe(`guide`)
    expect(screen.queryByTestId(`agent-session-view`)).toBeNull()
  })
})
