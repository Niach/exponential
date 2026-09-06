import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-740: the dock is the tab STRIP and nothing else — no panel, no resize,
// no fullscreen. What is pinned here: it always offers Chat, a chat run reads
// as "Chat" while an issue run leads with its identifier, the active tab comes
// from the URL (not from local state any more), the steer-store reaper keeps
// the running rows plus whatever the route is showing, and phones get none of
// it.

const mockState = vi.hoisted(() => ({
  running: [] as unknown[],
  isMobile: false,
  navigate: vi.fn(),
  params: {} as Record<string, string | undefined>,
  search: {} as { session?: string },
  matchRoute: vi.fn(() => false as unknown),
  retain: vi.fn(),
}))

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => mockState.navigate,
  useParams: () => mockState.params,
  useSearch: () => mockState.search,
  useMatchRoute: () => mockState.matchRoute,
}))

vi.mock(`@/hooks/use-agents-data`, () => ({
  useAgentsData: () => ({ running: mockState.running, isLoading: false }),
  rowPrState: () => null,
}))

vi.mock(`@/hooks/use-mobile`, () => ({
  useIsMobile: () => mockState.isMobile,
}))

// The real module drags the whole issue-detail tree into jsdom.
vi.mock(`@/components/issue-coding-rows`, () => ({
  sessionDisplayState: () => `running`,
}))

vi.mock(`@/lib/steer-session-store`, () => ({
  retainSteerSessions: mockState.retain,
}))

vi.mock(`@/hooks/use-kill-session`, () => ({
  useKillSession: () => ({
    canKill: false,
    requestKill: vi.fn(),
    dialog: null,
  }),
}))

vi.mock(`@/hooks/use-chrome-height-var`, () => ({
  useChromeHeightVar: () => () => {},
}))

import { AgentDock } from "@/components/agent-dock/agent-dock"

function issueRow(id: string) {
  return {
    session: {
      id,
      issueId: `i-${id}`,
      actionId: null,
      actionName: null,
      status: `running`,
      needsInput: false,
    },
    issue: { identifier: `EXP-1`, title: `Ship it` },
    board: undefined,
    user: undefined,
    mergeTarget: undefined,
    device: { label: `macbook`, online: true },
    paused: false,
  }
}

function chatRow(id: string) {
  return {
    session: {
      id,
      issueId: null,
      actionId: null,
      actionName: `Chat`,
      status: `running`,
      needsInput: false,
    },
    issue: undefined,
    board: undefined,
    user: undefined,
    mergeTarget: undefined,
    device: { label: `macbook`, online: true },
    paused: false,
  }
}

function mount() {
  return render(
    <AgentDock teamId="team-1" teamSlug="acme" currentUserId="user-1" />
  )
}

describe(`AgentDock`, () => {
  beforeEach(() => {
    mockState.running = []
    mockState.isMobile = false
    mockState.params = {}
    mockState.search = {}
    mockState.matchRoute = vi.fn(() => false as unknown)
    mockState.navigate.mockClear()
    mockState.retain.mockClear()
  })

  // The trailing button is "go to chat", not "go to THAT chat" — no search,
  // so the page lands on the newest running chat or the start card.
  it(`offers Chat even with nothing running`, () => {
    mount()
    const chat = screen.getByLabelText(`Chat`)
    expect(chat).toBeTruthy()
    expect(screen.queryAllByRole(`tab`)).toHaveLength(0)
    fireEvent.click(chat)
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/chat`,
      params: { teamSlug: `acme` },
    })
  })

  it(`titles a chat tab "Chat" and an issue tab by identifier`, () => {
    mockState.running = [issueRow(`s1`), chatRow(`s2`)]
    mount()
    const tabs = screen.getAllByRole(`tab`)
    expect(tabs).toHaveLength(2)
    expect(tabs[0]!.textContent).toContain(`EXP-1`)
    expect(tabs[0]!.textContent).toContain(`Ship it`)
    expect(tabs[1]!.textContent).toContain(`Chat`)
    expect(tabs[1]!.textContent).not.toContain(`EXP-1`)
  })

  it(`takes the active tab from the session route param`, () => {
    mockState.running = [issueRow(`s1`), issueRow(`s9`)]
    mockState.params = { teamSlug: `acme`, sessionId: `s9` }
    mount()
    const tabs = screen.getAllByRole(`tab`)
    expect(tabs[0]!.getAttribute(`aria-selected`)).toBe(`false`)
    expect(tabs[1]!.getAttribute(`aria-selected`)).toBe(`true`)
  })

  // A chat run has no session route of its own — the chat PAGE showing THAT
  // chat is what makes its tab active. With no `?session=` the page shows the
  // newest running chat, so only that tab lights up.
  it(`activates the newest chat tab on a bare chat route`, () => {
    mockState.running = [chatRow(`s2`), chatRow(`s1`)]
    mockState.matchRoute = vi.fn(
      (opts: { to: string }) => opts.to === `/t/$teamSlug/chat`
    ) as never
    mount()
    const tabs = screen.getAllByRole(`tab`)
    expect(tabs[0]!.getAttribute(`aria-selected`)).toBe(`true`)
    expect(tabs[1]!.getAttribute(`aria-selected`)).toBe(`false`)
  })

  it(`activates the chat tab named by ?session=`, () => {
    mockState.running = [chatRow(`s2`), chatRow(`s1`)]
    mockState.search = { session: `s1` }
    mockState.matchRoute = vi.fn(
      (opts: { to: string }) => opts.to === `/t/$teamSlug/chat`
    ) as never
    mount()
    const tabs = screen.getAllByRole(`tab`)
    expect(tabs[0]!.getAttribute(`aria-selected`)).toBe(`false`)
    expect(tabs[1]!.getAttribute(`aria-selected`)).toBe(`true`)
  })

  it(`opens a chat tab on the chat page, naming it`, () => {
    mockState.running = [chatRow(`s2`)]
    mockState.params = { teamSlug: `acme` }
    mount()
    fireEvent.click(screen.getByRole(`tab`))
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/chat`,
      params: { teamSlug: `acme` },
      search: { session: `s2` },
    })
  })

  it(`opens a tab's session on select`, () => {
    mockState.running = [issueRow(`s1`)]
    mockState.params = { teamSlug: `acme` }
    mount()
    fireEvent.click(screen.getByRole(`tab`))
    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s1` },
    })
  })

  it(`retains the running stores plus the one the route is showing`, () => {
    mockState.running = [issueRow(`s1`)]
    mockState.params = { teamSlug: `acme`, sessionId: `s-ended` }
    mount()
    const keep = mockState.retain.mock.calls.at(-1)![0] as Set<string>
    expect([...keep].sort()).toEqual([`s-ended`, `s1`])
  })

  it(`renders nothing on phones`, () => {
    mockState.isMobile = true
    mockState.running = [issueRow(`s1`)]
    const { container } = mount()
    expect(container.innerHTML).toBe(``)
  })
})
