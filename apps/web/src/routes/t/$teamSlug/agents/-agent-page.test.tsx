// #EXP-1732 (session-screen.json): a team agent's chat IS THE session
// screen over the viewer's conversation, with the agent's header parts and
// THE gear (its settings body); with no conversation yet the same frame
// (the tabs, the gear) over an empty chat whose composer (THE session
// composer) opens one with `sessions.send({agentId})` — md+ inside the
// Dock, a phone in THE floating composer capsule (FEED-160).
// Lives under a `-` prefix so the route generator ignores it.
import type { ComponentType, ReactNode } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  isMobile: { current: false },
  conversation: { current: null as Record<string, unknown> | null },
  isMember: { current: true },
  screen: { current: null as Record<string, unknown> | null },
  search: { current: {} as Record<string, unknown> },
  navigate: vi.fn(),
}))

vi.mock(`@tanstack/react-router`, () => ({
  createFileRoute: () => (options: Record<string, unknown>) => ({
    ...options,
    useParams: () => ({ teamSlug: `acme`, agentId: `agent-1` }),
    useSearch: () => mocks.search.current,
  }),
  useNavigate: () => mocks.navigate,
  redirect: vi.fn(),
}))
vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => mocks.isMobile.current,
  ChatDock: ({ composer, phoneClearance }: { composer?: ReactNode; phoneClearance?: boolean }) => (
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
    agent: {
      id: `agent-1`,
      teamId: `t1`,
      name: `Triage`,
      icon: null,
      color: null,
      triggers: [
        { id: `tr0`, kind: `event`, name: `issue.created`, enabled: false },
        { id: `tr1`, kind: `event`, name: `issue.created`, enabled: true },
      ],
    },
    isReady: true,
  }),
  useAllAgentRows: () => [],
  useMyAgentConversation: () => mocks.conversation.current,
}))
vi.mock(`@/hooks/use-session-tree`, () => ({
  useTeamSessions: () => ({ rows: mocks.conversation.current ? [mocks.conversation.current] : [] }),
  useSessionChat: (rootId: string | null) =>
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
vi.mock(`@/components/team/agents-section`, () => ({
  AgentSettingsPage: ({ agent }: { agent: { id: string } }) => (
    <div data-testid="agent-settings-body" data-agent={agent.id} />
  ),
}))
vi.mock(`@/hooks/use-results-tab`, () => ({
  useResultsTab: () => ({ results: [], cardFor: () => null, loading: false }),
}))
vi.mock(`@/components/chat/session-screen-bodies`, () => ({
  SessionIssuesTab: () => <div data-testid="issues-tab" />,
  SessionSessionsTab: () => <div data-testid="sessions-tab" />,
  SessionResultsTab: () => <div data-testid="results-tab" />,
}))
vi.mock(`@/components/chat/session-body`, () => ({ BackStackButton: () => null }))
vi.mock(`@/components/chat/chat-screen`, () => ({
  validateChatSearch: () => ({}),
  ChatScreen: (props: Record<string, unknown>) => {
    mocks.screen.current = props
    return <div data-testid="chat-screen" />
  },
}))
vi.mock(`@/hooks/use-follow-scroll`, () => ({
  useFollowScroll: () => ({
    scrollRef: { current: null },
    contentRef: { current: null },
    onScroll: () => {},
    atBottom: true,
    jumpToBottom: () => {},
    hold: () => {},
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
vi.mock(`@/components/chat/chat-markdown`, () => ({ renderChatText: (text: string) => text }))
// #FEED-239: THE session screen menu's Agent settings entry (no gear).
vi.mock(`@/components/chat/chat-menu`, () => ({
  AgentHeaderMenu: ({ onSettings }: { onSettings: () => void }) => (
    <button type="button" data-testid="session-menu-settings" onClick={onSettings} />
  ),
}))
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
  mocks.screen.current = null
  mocks.search.current = {}
  mocks.navigate.mockReset()
})

describe(`the agent chat = THE session screen`, () => {
  it(`renders the session screen over the conversation with the agent's header parts and settings`, () => {
    mocks.conversation.current = conversationRow
    render(<Page />)
    expect(screen.getByTestId(`chat-screen`)).toBeTruthy()
    const props = mocks.screen.current!
    expect((props.root as { id: string }).id).toBe(`c1`)
    const agent = props.agent as { name: string; assistant: boolean; trigger: string | null; settings: unknown }
    expect(agent.name).toBe(`Triage`)
    expect(agent.assistant).toBe(false)
    // session-screen.json: the subtitle's trigger = the FIRST ENABLED one.
    expect(agent.trigger).toBeTruthy()
    expect(agent.settings).toBeTruthy()
    expect(screen.queryByTestId(`session-composer`)).toBeNull()
  })
})

describe(`the agent chat with no conversation yet`, () => {
  it(`md+: the frame's tabs + the menu (no gear), the opening composer inside the Dock, no floating capsule`, () => {
    render(<Page />)
    const tabs = screen.getByTestId(`session-tabs`)
    expect([...tabs.querySelectorAll(`[data-tab]`)].map((node) => node.getAttribute(`data-tab`))).toEqual([
      `chat`,
      `issues`,
      `sessions`,
      `results`,
    ])
    expect(screen.queryByTestId(`session-gear`)).toBeNull()
    expect(screen.getByTestId(`session-menu-settings`)).toBeTruthy()
    // The Chat tab alone = the chat full width (no beside split).
    expect(screen.queryByTestId(`session-beside`)).toBeNull()
    const dock = screen.getByTestId(`dock`)
    const composer = dock.querySelector(`[data-testid="session-composer"]`)
    expect(composer?.getAttribute(`data-session`)).toBe(`null`)
    expect(composer?.getAttribute(`data-agent`)).toBe(`agent-1`)
    expect(screen.queryByTestId(`floating-composer`)).toBeNull()
  })

  it(`md+: another tab = the beside rule (the chat pane + the tab pane)`, () => {
    mocks.search.current = { tab: `sessions` }
    render(<Page />)
    const beside = screen.getByTestId(`session-beside`)
    expect(beside.querySelector(`[data-testid="dock"]`)).toBeTruthy()
    expect(screen.getByTestId(`session-pane`).querySelector(`[data-testid="sessions-tab"]`)).toBeTruthy()
  })

  it(`the menu's Agent settings opens the agent settings body as an opened item`, () => {
    render(<Page />)
    fireEvent.click(screen.getByTestId(`session-menu-settings`))
    expect(mocks.navigate).toHaveBeenCalledWith(
      expect.objectContaining({ search: expect.objectContaining({ open: `settings` }) })
    )
    mocks.search.current = { open: `settings` }
    render(<Page />)
    const pane = screen.getByTestId(`session-pane`)
    expect(pane.querySelector(`[data-testid="agent-settings-body"]`)?.getAttribute(`data-agent`)).toBe(`agent-1`)
    // #FEED-239: wide = THE collapse button leads the pane, no Back.
    expect(screen.queryByTestId(`session-item-back`)).toBeNull()
    expect(screen.getByTestId(`session-collapse`)).toBeTruthy()
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
