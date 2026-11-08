import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Notification, Team } from "@/db/schema"

// #FEED-191: the compact rows are the wide popover's: a press reports back so
// it closes, and every row carries its time.

const data = vi.hoisted(() => ({
  notifications: [] as unknown[],
  teams: [] as unknown[],
  sessions: new Map<string, unknown>(),
}))
const navigate = vi.hoisted(() => vi.fn())

vi.mock(`@tanstack/react-router`, () => ({
  Link: ({ children }: { children: React.ReactNode }) => <a>{children}</a>,
  useNavigate: () => navigate,
  useParams: () => ({ teamSlug: `acme` }),
}))
vi.mock(`@/lib/collections`, () => ({
  notificationCollection: { tag: `notifications` },
  issueCollection: { tag: `issues` },
  boardCollection: { tag: `boards` },
  teamCollection: { tag: `teams` },
  sessionCollection: { tag: `sessions`, get: (id: string) => data.sessions.get(id) },
}))
vi.mock(`@/hooks/use-session-tree`, () => ({
  useDigestMessages: () => ({ messages: [] }),
}))
vi.mock(`@/hooks/use-session`, () => ({
  useSession: () => ({ data: { user: { id: `me` } } }),
}))
vi.mock(`@tanstack/react-db`, () => ({
  inArray: () => true,
  useLiveQuery: (build: (query: unknown) => unknown) => {
    let tag: string | null = null
    const chain = {
      orderBy: () => chain,
      where: () => chain,
    }
    const result = build({
      from: (source: Record<string, { tag: string }>) => {
        tag = Object.values(source)[0]!.tag
        return chain
      },
    })
    if (result === undefined) return { data: undefined }
    if (tag === `notifications`) return { data: data.notifications }
    if (tag === `teams`) return { data: data.teams }
    return { data: [] }
  },
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { notifications: { markRead: { mutate: vi.fn() }, markReadByIssue: { mutate: vi.fn() } } },
}))

import { InboxView } from "@/components/inbox/inbox-view"

const blocked = (id: string, sessionId: string): Notification =>
  ({
    id,
    type: `session_blocked`,
    issueId: null,
    teamId: `t1`,
    sessionId,
    title: `Run ${sessionId} is blocked`,
    body: null,
    readAt: null,
    createdAt: new Date(`2026-10-0${id === `n1` ? 2 : 1}T10:00:00Z`),
  }) as unknown as Notification

const message = (id: string, sessionId: string | null): Notification =>
  ({
    id,
    type: `agent_message`,
    issueId: null,
    teamId: `t1`,
    sessionId,
    title: `Your Assistant needs a decision`,
    body: `Ship it today?`,
    readAt: null,
    createdAt: new Date(`2026-10-03T10:00:00Z`),
  }) as unknown as Notification

describe(`InboxView message rows`, () => {
  beforeEach(() => {
    data.sessions = new Map()
    data.notifications = [blocked(`n1`, `s1`), blocked(`n2`, `s2`)]
    data.teams = [{ id: `t1`, slug: `acme`, name: `Acme` } as Team]
    navigate.mockClear()
  })

  it(`reports a press and shows the time, read or unread`, () => {
    const onRowOpen = vi.fn()
    render(<InboxView compact onRowOpen={onRowOpen} />)
    const rows = screen.getAllByTestId(`inbox-row-session_blocked`)
    expect(rows[0]!.textContent).toMatch(/\d+[mhdwy]|now/)
    fireEvent.click(rows[0]!)
    expect(onRowOpen).toHaveBeenCalledTimes(1)
  })

  it(`opens the run beside the list with the inbox origin`, () => {
    render(<InboxView compact from="inbox" />)
    fireEvent.click(screen.getAllByTestId(`inbox-row-session_blocked`)[0]!)
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s1` },
      search: { from: `inbox` },
    })
  })

  // #EXP-1456 (inbox-route.json): an agent's decision names its session.
  it(`opens the viewer's own agent conversation on the agent's page`, () => {
    data.notifications = [message(`n3`, `conv`)]
    data.sessions.set(`conv`, { id: `conv`, rootId: `conv`, agentId: `assistant`, userId: `me` })
    render(<InboxView compact from="inbox" />)
    fireEvent.click(screen.getByTestId(`inbox-row-agent_message`))
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/agents/$agentId`,
      params: { teamSlug: `acme`, agentId: `assistant` },
      search: { from: `inbox` },
    })
  })

  it(`opens a chat's ask through the session route`, () => {
    data.notifications = [message(`n4`, `chat`)]
    data.sessions.set(`chat`, { id: `chat`, rootId: `chat`, agentId: null, userId: `me` })
    render(<InboxView compact from="inbox" />)
    fireEvent.click(screen.getByTestId(`inbox-row-agent_message`))
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `chat` },
      search: { from: `inbox` },
    })
  })

  it(`opens nothing for an agent message without a session`, () => {
    data.notifications = [message(`n5`, null)]
    render(<InboxView compact from="inbox" />)
    fireEvent.click(screen.getByTestId(`inbox-row-agent_message`))
    expect(navigate).not.toHaveBeenCalled()
  })

  // #EXP-1534 (inbox-route.json): a device's signed-out login, team-less.
  const signedOut = (deviceId: string | null): Notification =>
    ({
      id: `n6`,
      type: `agent_signed_out`,
      issueId: null,
      teamId: null,
      sessionId: null,
      deviceId,
      title: `Claude Code account d@acme.dev is signed out on mint`,
      body: null,
      readAt: null,
      createdAt: new Date(`2026-10-04T10:00:00Z`),
    }) as unknown as Notification

  it(`lists a signed-out login in a team's stream and opens its device's settings`, () => {
    data.notifications = [signedOut(`dev-row`)]
    render(<InboxView compact teamId="t1" from="inbox" />)
    fireEvent.click(screen.getByTestId(`inbox-row-agent_signed_out`))
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/devices`,
      params: { teamSlug: `acme` },
      search: { device: `dev-row` },
    })
  })

  it(`opens nothing for a removed device`, () => {
    data.notifications = [signedOut(null)]
    render(<InboxView compact from="inbox" />)
    fireEvent.click(screen.getByTestId(`inbox-row-agent_signed_out`))
    expect(navigate).not.toHaveBeenCalled()
  })
})
