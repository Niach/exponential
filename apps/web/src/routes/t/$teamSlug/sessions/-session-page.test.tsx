// EXP-1293 / EXP-1316 / EXP-1733: `/sessions/$id` — a ROOT is its chat; a
// child REDIRECTS into its root (#EXP-1732: `?tab=sessions&open=session&session=<id>`, `from` kept);
// a session under an
// agent's conversation (NEW-2: a root with `agentId`) keeps its own screen: back to the agent, the session's name,
// no header Stop (Chats), the feed and the reply box. Lives under a `-` prefix so
// the route generator ignores it.
import type { ComponentType, ReactNode } from "react"
import { act, render, screen } from "@testing-library/react"
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
      data-chat={params.sessionId}
      data-tab={search?.tab}
      data-open={search?.open}
      data-session={search?.session}
      data-app={params.appSlug}
      data-from={search?.from}
      data-back={search?.back}
    />
  ),
  useNavigate: () => vi.fn(),
  Link: ({
    children,
    params,
    ...rest
  }: {
    children: ReactNode
    params?: Record<string, string>
    "data-testid"?: string
  }) => (
    <a data-testid={rest[`data-testid`]} data-chat={params?.sessionId}>
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
  useSessionChat: (id: string) => ({
    view: null,
    root: mocks.row.current?.id === id && mocks.row.current?.rootId === id ? mocks.row.current : null,
    isReady: true,
    titleReady: true,
  }),
  useSessionMessages: () => [],
  useDigestMessages: () => ({ messages: [], isReady: true, loaded: new Set() }),
}))
vi.mock(`@/hooks/use-agent-row`, () => ({
  useAgentRow: (id: string | null) => ({
    agent: id === `agent-1` ? { id, name: `Triage` } : null,
    isReady: true,
  }),
  useAllAgentRows: () => [],
}))
vi.mock(`@/hooks/use-apps`, () => ({
  useApps: () => ({ apps: [{ id: `app-1`, slug: `working-hours` }], isReady: true }),
  useAppName: () => null,
}))
vi.mock(`@/hooks/use-session`, () => ({
  useSession: () => ({ data: { user: { id: `u1` } } }),
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
  it(`redirects a chat's session into its chat, keeping from`, () => {
    mocks.root.current = { id: `s1`, agentId: null, rootId: `s1`, teamId: `t1` }
    mocks.search.current = { from: `inbox` }
    render(<Page />)
    const target = screen.getByTestId(`navigate`)
    expect(target.getAttribute(`data-to`)).toBe(`/t/$teamSlug/sessions/$sessionId`)
    expect(target.getAttribute(`data-chat`)).toBe(`s1`)
    expect(target.getAttribute(`data-tab`)).toBe(`sessions`)
    expect(target.getAttribute(`data-open`)).toBe(`session`)
    expect(target.getAttribute(`data-session`)).toBe(`s2`)
    expect(target.getAttribute(`data-from`)).toBe(`inbox`)
    expect(screen.queryByTestId(`agent-session-view`)).toBeNull()
  })

  it(`S-3: keeps the back stack through the redirect`, () => {
    mocks.root.current = { id: `s1`, agentId: null, rootId: `s1`, teamId: `t1` }
    mocks.search.current = { back: `s9,s1` }
    render(<Page />)
    expect(screen.getByTestId(`navigate`).getAttribute(`data-back`)).toBe(`s9,s1`)
  })

  it(`keeps an agent's session on its own screen, back to the agent`, () => {
    render(<Page />)
    const back = screen.getByTestId(`session-back`)
    expect(back.textContent).toBe(`Triage`)
    expect(screen.getByTestId(`session-title`).textContent).toBe(`Profile the launch path`)
    const composer = screen.getByTestId(`session-composer`)
    expect(composer.getAttribute(`data-session`)).toBe(`s2`)
    expect(composer.textContent).toBe(`Type / for commands`)
  })

  // #EXP-1732: a session under MY conversation with the agent opens in
  // place on the agent's screen (its Sessions tab).
  it(`sends a session of my agent conversation to the agent's Sessions tab`, () => {
    mocks.root.current = { id: `s1`, agentId: `agent-1`, rootId: `s1`, teamId: `t1`, userId: `u1` }
    render(<Page />)
    const target = screen.getByTestId(`navigate`)
    expect(target.getAttribute(`data-to`)).toBe(`/t/$teamSlug/agents/$agentId`)
    expect(target.getAttribute(`data-tab`)).toBe(`sessions`)
    expect(target.getAttribute(`data-open`)).toBe(`session`)
    expect(target.getAttribute(`data-session`)).toBe(`s2`)
  })

  // Chat 58cb32df (chats.json STOP): an agent's tree has no header Stop
  // pill; the composer's turn stop and the agent menu's Stop remain.
  it(`has no header Stop under an agent, even while live`, () => {
    render(<Page />)
    expect(screen.queryByTestId(`session-stop`)).toBeNull()
    expect(mocks.stop).not.toHaveBeenCalled()
  })

  it(`has no Stop once the session ended`, () => {
    mocks.row.current = row({ status: `ended`, busy: false })
    render(<Page />)
    expect(screen.queryByTestId(`session-stop`)).toBeNull()
    expect(screen.getByTestId(`agent-session-view`)).toBeTruthy()
  })

  it(`says so when the session is not there, after the sync grace`, () => {
    vi.useFakeTimers()
    try {
      mocks.row.current = null
      render(<Page />)
      expect(screen.queryByText(`Chat not found`)).toBeNull()
      act(() => {
        vi.advanceTimersByTime(4_000)
      })
      expect(screen.queryByTestId(`agent-session-view`)).toBeNull()
      expect(screen.getByText(`Chat not found`)).toBeTruthy()
    } finally {
      vi.useRealTimers()
    }
  })

  // EXP-1733: a ROOT's agent conversation opens the agent's page.
  it(`sends an agent's conversation root to the agent page`, () => {
    mocks.row.current = { id: `s2`, rootId: `s2`, agentId: `agent-1`, teamId: `t1` }
    render(<Page />)
    expect(screen.getByTestId(`navigate`).getAttribute(`data-to`)).toBe(`/t/$teamSlug/agents/$agentId`)
  })

  // #EXP-1732: an app's editor ROOT opens its app editor, keeping tab/open.
  it(`sends an app's editor root to the app editor, keeping tab and open`, () => {
    mocks.row.current = { id: `s2`, rootId: `s2`, appId: `app-1`, teamId: `t1` }
    mocks.search.current = { tab: `sessions`, open: `session`, session: `s7` }
    render(<Page />)
    const target = screen.getByTestId(`navigate`)
    expect(target.getAttribute(`data-to`)).toBe(`/t/$teamSlug/app-editor/$appSlug`)
    expect(target.getAttribute(`data-app`)).toBe(`working-hours`)
    expect(target.getAttribute(`data-tab`)).toBe(`sessions`)
    expect(target.getAttribute(`data-open`)).toBe(`session`)
    expect(target.getAttribute(`data-session`)).toBe(`s7`)
  })

  // EXP-1291: the PR body's footer links `sessions/<rootId>?view=guide`.
  it(`S-3: ?view=guide (old PR footers) opens the chat's conversation`, () => {
    mocks.search.current = { view: `guide` }
    render(<Page />)
    const target = screen.getByTestId(`navigate`)
    expect(target.getAttribute(`data-to`)).toBe(`/t/$teamSlug/sessions/$sessionId`)
    expect(target.getAttribute(`data-chat`)).toBe(`s1`)
    expect(target.getAttribute(`data-tab`)).toBeNull()
    expect(screen.queryByTestId(`agent-session-view`)).toBeNull()
  })
})
