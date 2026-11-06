import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/collections`, () => ({ sessionCollection: { get: () => undefined }, deviceCollection: {} }))
vi.mock(`@/hooks/use-session-tree`, () => ({}))
vi.mock(`@/hooks/use-team-data`, () => ({}))
vi.mock(`@/components/thread/threads-list`, () => ({}))
vi.mock(`@tanstack/react-router`, () => ({ useNavigate: () => vi.fn() }))

const { needsYouDestination, withStuckRows } = await import(`./use-needs-you`)

const sessions = new Map<string, { id: string; rootId: string; agentId: string | null; userId: string | null }>([
  [`r`, { id: `r`, rootId: `r`, agentId: null, userId: `me` }],
  [`c`, { id: `c`, rootId: `r`, agentId: null, userId: `me` }],
  [`a`, { id: `a`, rootId: `a`, agentId: `agent`, userId: `me` }],
  [`ac`, { id: `ac`, rootId: `a`, agentId: null, userId: `me` }],
])
const lookup = (id: string) => sessions.get(id)
const go = (row: { sessionId: string; rootId: string; opens: `plan` | `card` | `results` | `chat` }) =>
  needsYouDestination({ messageId: `m`, ...row }, `acme`, lookup, `me`)

describe(`needsYouDestination`, () => {
  it(`a plan = its deep link on the card's own session`, () => {
    expect(go({ sessionId: `c`, rootId: `r`, opens: `plan` })).toEqual({
      to: `/t/$teamSlug/sessions/$sessionId/plan/$messageId`,
      params: { teamSlug: `acme`, sessionId: `c`, messageId: `m` },
    })
  })

  it(`a child's question = the root's Conversation, scrolled to the card`, () => {
    expect(go({ sessionId: `c`, rootId: `r`, opens: `card` })).toEqual({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug: `acme`, threadId: `r` },
      search: { card: `m` },
    })
  })

  it(`a question in an agent conversation or its child = the agent's chat`, () => {
    const agent = {
      to: `/t/$teamSlug/agents/$agentId`,
      params: { teamSlug: `acme`, agentId: `agent` },
      search: { card: `m` },
    }
    expect(go({ sessionId: `a`, rootId: `a`, opens: `card` })).toEqual(agent)
    expect(go({ sessionId: `ac`, rootId: `a`, opens: `card` })).toEqual(agent)
  })

  it(`a PR = the Results tab; an agent conversation's = its chat`, () => {
    expect(go({ sessionId: `r`, rootId: `r`, opens: `results` })).toEqual({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug: `acme`, threadId: `r` },
      search: { tab: `results` },
    })
    expect(go({ sessionId: `a`, rootId: `a`, opens: `chat` })).toEqual({
      to: `/t/$teamSlug/agents/$agentId`,
      params: { teamSlug: `acme`, agentId: `agent` },
    })
  })
})

// #EXP-1710 (merge-train.json `stuckRow`): the stuck PRs folded into the rows.
describe(`withStuckRows`, () => {
  const now = Date.parse(`2027-01-15T10:00:00.000Z`)
  const decision = {
    messageId: `q1`, sessionId: `r1`, rootId: `r1`, kind: `question` as const, opens: `card` as const,
    title: `Which?`, chat: `Palette`, meta: ``, age: `1h`, createdAt: `2027-01-15T09:00:00.000Z`,
  }
  const ready = {
    messageId: `p1`, sessionId: `r1`, rootId: `r1`, kind: `pr` as const, opens: `results` as const,
    title: `Ship`, chat: `Palette`, meta: `#12`, age: `3h`, createdAt: `2027-01-15T07:00:00.000Z`,
  }
  const pr = (over: Record<string, unknown>) =>
    ({
      id: `p1`, number: 12, state: `open`, rootSessionId: `r1`, sessionId: `c1`, mergeState: `held`,
      mergeHold: `bounces`, createdAt: `2027-01-15T07:00:00.000Z`, ...over,
    }) as never
  const sessions = new Map([[`c1`, { deviceId: `mac`, userId: `me` }]])
  const chats = new Map([[`r1`, `Palette`]])
  const devices = (lastSeenAt: string) => [{ deviceId: `mac`, userId: `me`, label: `Mac`, lastSeenAt }] as never

  it(`a PR held by the bounces replaces its ready row, under the decisions`, () => {
    const rows = withStuckRows([decision, ready], {
      pullRequests: [pr({})], sessions, chats, devices: devices(`2027-01-15T09:59:50.000Z`), now, owner: true,
    })
    expect(rows.map((row) => [row.messageId, row.title, row.meta])).toEqual([
      [`q1`, `Which?`, ``],
      [`p1`, `Auto-merge stuck on #12`, `3 bounces`],
    ])
    expect(rows[1]!.stuck?.actions).toEqual([`retry`, `merge-now`, `close-pr`])
  })

  it(`a conflict waits for an offline device 2 h, then is a row; a member gets no Merge now`, () => {
    const conflict = pr({ mergeState: `conflict`, mergeHold: null })
    const at = (seen: string) =>
      withStuckRows([], { pullRequests: [conflict], sessions, chats, devices: devices(seen), now, owner: false })
    expect(at(`2027-01-15T09:00:00.000Z`)).toEqual([])
    const [row] = at(`2027-01-15T08:00:00.000Z`)
    expect(row?.meta).toBe(`Device offline`)
    expect(row?.stuck?.actions).toEqual([`retry`, `close-pr`])
  })

  it(`a person's Hold is no row`, () => {
    expect(
      withStuckRows([], { pullRequests: [pr({ mergeHold: `person` })], sessions, chats, devices: [], now, owner: true })
    ).toEqual([])
  })
})
