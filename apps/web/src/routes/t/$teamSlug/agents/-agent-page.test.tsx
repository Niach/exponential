// S-3 (chats.json CHAT MODE): a team agent's chat IS the thread screen over
// the viewer's conversation, with the agent's header parts; with no
// conversation yet, the empty screen's composer (THE session composer)
// opens one with `sessions.send({agentId})` — md+ inside the Dock, a phone
// in THE floating composer capsule (FEED-160).
// Lives under a `-` prefix so the route generator ignores it.
import type { ComponentType, ReactNode } from "react"
import { render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  isMobile: { current: false },
  conversation: { current: null as Record<string, unknown> | null },
  isMember: { current: true },
  screen: { current: null as Record<string, unknown> | null },
}))

vi.mock(`@tanstack/react-router`, () => ({
  createFileRoute: () => (options: Record<string, unknown>) => ({
    ...options,
    useParams: () => ({ teamSlug: `acme`, agentId: `agent-1` }),
    useSearch: () => ({}),
  }),
  useNavigate: () => vi.fn(),
  redirect: vi.fn(),
}))
vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => mocks.isMobile.current,
  ThreadDock: ({ composer, phoneClearance }: { composer?: ReactNode; phoneClearance?: boolean }) => (
    <div data-testid="dock" data-clearance={String(!!phoneClearance)}>
      {composer}
    </div>
  ),
  FloatingComposer: ({
    placeholder,
    above,
    children,
  }: {
    placeholder: string
    above?: ReactNode
    children: ReactNode
  }) => (
    <div data-testid="floating-composer" data-placeholder={placeholder}>
      {above}
      {children}
    </div>
  ),
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBySlug: () => ({ id: `t1`, slug: `acme` }),
  useTeamUsers: () => ({ users: [] }),
}))
vi.mock(`@/hooks/use-team-permissions`, () => ({
  useTeamPermissions: () => ({ isMember: mocks.isMember.current }),
}))
vi.mock(`@/hooks/use-session`, () => ({
  useSession: () => ({ data: { user: { id: `u1` } } }),
}))
vi.mock(`@/hooks/use-agent-row`, () => ({
  useAgentRow: () => ({
    agent: { id: `agent-1`, teamId: `t1`, name: `Triage`, icon: null, color: null },
    isReady: true,
  }),
  useAllAgentRows: () => [],
}))
vi.mock(`@/hooks/use-session-tree`, () => ({
  useTeamSessions: () => ({ rows: mocks.conversation.current ? [mocks.conversation.current] : [] }),
  useSessionThread: (rootId: string | null) =>
    rootId && mocks.conversation.current
      ? {
          view: { title: `x` },
          root: mocks.conversation.current,
          rows: [],
          titles: new Map(),
          pullRequests: [],
          messagePage: {},
        }
      : { view: null, root: null, rows: [], titles: new Map(), pullRequests: [], messagePage: {} },
}))
vi.mock(`@/components/thread/thread-screen`, () => ({
  validateThreadSearch: () => ({}),
  ThreadScreen: (props: Record<string, unknown>) => {
    mocks.screen.current = props
    return <div data-testid="thread-screen" />
  },
}))
vi.mock(`@/hooks/use-follow-scroll`, () => ({
  useFollowScroll: () => ({
    scrollRef: { current: null },
    contentRef: { current: null },
    onScroll: () => {},
    atBottom: true,
    jumpToBottom: () => {},
  }),
}))
vi.mock(`@/lib/page-title`, () => ({
  pageTitle: (...parts: string[]) => parts.join(` `),
  usePageTitle: () => {},
}))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: { sessions: { send: { mutate: vi.fn() } } } }))
vi.mock(`@/components/agent-session`, () => ({
  SessionLiveTail: () => null,
  useSessionDock: () => ({ sources: {}, renderSheet: () => null, renderAction: () => null }),
}))
vi.mock(`@/components/session-composer`, () => ({
  SessionComposer: ({ opaque, sessionId, agentId }: { opaque?: boolean; sessionId: string | null; agentId?: string }) => (
    <div
      data-testid="session-composer"
      data-opaque={String(!!opaque)}
      data-session={String(sessionId)}
      data-agent={agentId}
    />
  ),
}))
vi.mock(`@/components/thread/thread-markdown`, () => ({ renderThreadText: (text: string) => text }))
vi.mock(`@/components/thread/chat-menu`, () => ({ AgentHeaderMenu: () => null }))
vi.mock(`@/components/team/mobile-detail-header`, () => ({
  MobileDetailHeader: () => <div data-testid="mobile-header" />,
  HEADER_BUTTON_CLASS: ``,
}))
vi.mock(`@/components/voice-call`, () => ({
  CallButton: () => null,
  VoiceCallOverlay: () => null,
  useVoiceCall: () => ({ call: { active: false, start: vi.fn() } }),
  useVoiceCallAvailability: () => ({ device: null }),
}))

const { Route } = await import(`@/routes/t/$teamSlug/agents/$agentId`)
const Page = (Route as unknown as { component: ComponentType }).component

const conversationRow = {
  id: `c1`,
  teamId: `t1`,
  agentId: `agent-1`,
  userId: `u1`,
  parentId: null,
  status: `idle`,
  busy: false,
}

beforeEach(() => {
  mocks.isMobile.current = false
  mocks.conversation.current = null
  mocks.isMember.current = true
})

beforeEach(() => {
  mocks.screen.current = null
})

describe(`the agent chat = THE thread screen`, () => {
  it(`renders the thread screen over the conversation with the agent's header`, () => {
    mocks.conversation.current = conversationRow
    render(<Page />)
    expect(screen.getByTestId(`thread-screen`)).toBeTruthy()
    const props = mocks.screen.current!
    expect((props.root as { id: string }).id).toBe(`c1`)
    expect((props.agent as { name: string }).name).toBe(`Triage`)
    expect(screen.queryByTestId(`session-composer`)).toBeNull()
  })
})

describe(`the agent chat with no conversation yet`, () => {
  it(`md+: the opening composer sits inside the Dock, no floating capsule`, () => {
    render(<Page />)
    const dock = screen.getByTestId(`dock`)
    const composer = dock.querySelector(`[data-testid="session-composer"]`)
    expect(composer?.getAttribute(`data-session`)).toBe(`null`)
    expect(composer?.getAttribute(`data-agent`)).toBe(`agent-1`)
    expect(screen.queryByTestId(`floating-composer`)).toBeNull()
  })

  it(`phone: the opening composer floats in THE capsule`, () => {
    mocks.isMobile.current = true
    render(<Page />)
    const floating = screen.getByTestId(`floating-composer`)
    expect(floating.getAttribute(`data-placeholder`)).toBe(`Message Triage`)
    expect(
      floating.querySelector(`[data-testid="session-composer"]`)?.getAttribute(`data-opaque`)
    ).toBe(`true`)
  })

  it(`a non-member gets no composer on a phone`, () => {
    mocks.isMobile.current = true
    mocks.isMember.current = false
    render(<Page />)
    expect(screen.queryByTestId(`floating-composer`)).toBeNull()
  })
})
