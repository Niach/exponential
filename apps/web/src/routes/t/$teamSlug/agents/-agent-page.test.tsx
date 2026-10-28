// FEED-160: a team agent's page replies EXACTLY like a thread — on a phone
// THE floating composer capsule (`FloatingComposer`) over a Dock that carries
// only the work bar's clearance; md+ keeps the composer inside the Dock.
// Lives under a `-` prefix so the route generator ignores it.
import type { ComponentType, ReactNode } from "react"
import { render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  isMobile: { current: false },
  conversation: { current: null as Record<string, unknown> | null },
  isMember: { current: true },
}))

vi.mock(`@tanstack/react-router`, () => ({
  createFileRoute: () => (options: Record<string, unknown>) => ({
    ...options,
    useParams: () => ({ teamSlug: `acme`, agentId: `agent-1` }),
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
}))
vi.mock(`@/hooks/use-session-tree`, () => ({
  useTeamSessions: () => ({ rows: mocks.conversation.current ? [mocks.conversation.current] : [] }),
  useTreeMessages: () => [],
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
  SessionComposer: ({ opaque, hideReplies }: { opaque?: boolean; hideReplies?: boolean }) => (
    <div data-testid="session-composer" data-opaque={String(!!opaque)} data-hide-replies={String(!!hideReplies)} />
  ),
  SessionComposerReplies: ({ sessionId }: { sessionId: string }) => (
    <div data-testid="composer-replies" data-session={sessionId} />
  ),
}))
vi.mock(`@/components/thread/agent-channel`, () => ({
  AgentChannel: () => <div data-testid="agent-channel" />,
  AgentComposer: ({ opaque }: { opaque?: boolean }) => (
    <div data-testid="agent-composer" data-opaque={String(!!opaque)} />
  ),
  useAgentChannelRows: () => ({ rows: [], tabs: [] }),
  useAnswerMessage: () => ({ answer: vi.fn(), pending: new Set() }),
}))
vi.mock(`@/components/thread/thread-markdown`, () => ({ renderThreadText: (text: string) => text }))
vi.mock(`@/components/thread/chat-menu`, () => ({ AgentHeaderMenu: () => null }))
vi.mock(`@/components/thread/chat-header`, () => ({ AgentHeaderTitle: () => null }))
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

describe(`the agent page's composer`, () => {
  it(`md+: the composer sits inside the Dock, no floating capsule`, () => {
    mocks.conversation.current = conversationRow
    render(<Page />)
    const dock = screen.getByTestId(`dock`)
    expect(dock.getAttribute(`data-clearance`)).toBe(`false`)
    expect(dock.querySelector(`[data-testid="session-composer"]`)).not.toBeNull()
    expect(screen.queryByTestId(`floating-composer`)).toBeNull()
  })

  it(`phone: THE floating composer (as the thread page) over a clearance-only Dock`, () => {
    mocks.isMobile.current = true
    mocks.conversation.current = conversationRow
    render(<Page />)
    const dock = screen.getByTestId(`dock`)
    expect(dock.getAttribute(`data-clearance`)).toBe(`true`)
    expect(dock.querySelector(`[data-testid="session-composer"]`)).toBeNull()
    const floating = screen.getByTestId(`floating-composer`)
    expect(floating.getAttribute(`data-placeholder`)).toBe(`Message Triage`)
    const composer = floating.querySelector(`[data-testid="session-composer"]`)
    expect(composer?.getAttribute(`data-opaque`)).toBe(`true`)
    expect(composer?.getAttribute(`data-hide-replies`)).toBe(`true`)
    expect(floating.querySelector(`[data-testid="composer-replies"]`)?.getAttribute(`data-session`)).toBe(`c1`)
  })

  it(`phone, no conversation yet: the opening composer floats the same way`, () => {
    mocks.isMobile.current = true
    render(<Page />)
    expect(screen.getByTestId(`dock`).getAttribute(`data-clearance`)).toBe(`true`)
    const floating = screen.getByTestId(`floating-composer`)
    expect(floating.querySelector(`[data-testid="agent-composer"]`)?.getAttribute(`data-opaque`)).toBe(`true`)
    expect(floating.querySelector(`[data-testid="composer-replies"]`)).toBeNull()
  })

  it(`a non-member gets no composer on a phone`, () => {
    mocks.isMobile.current = true
    mocks.isMember.current = false
    render(<Page />)
    expect(screen.queryByTestId(`floating-composer`)).toBeNull()
  })
})
