import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/collections`, () => ({ sessionCollection: { get: () => undefined }, deviceCollection: {} }))
vi.mock(`@/hooks/use-session-tree`, () => ({}))
vi.mock(`@/hooks/use-team-data`, () => ({}))
vi.mock(`@/components/thread/threads-list`, () => ({}))
vi.mock(`@tanstack/react-router`, () => ({ useNavigate: () => vi.fn() }))

const { needsYouDestination, prDeviceOfflineSince, withApprovals, withStuckRows } = await import(`./use-needs-you`)

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
      mergeHold: `bounces`, createdAt: `2027-01-15T07:00:00.000Z`, updatedAt: `2027-01-15T07:30:00.000Z`, ...over,
    }) as never
  const sessions = new Map([[`c1`, { deviceId: `mac`, userId: `me` }]])
  const chats = new Map([[`r1`, `Palette`]])
  const devices = (lastSeenAt: string | null) => [{ deviceId: `mac`, userId: `me`, label: `Mac`, lastSeenAt }] as never

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

  it(`an unknown last-seen (no devices row, or none synced yet) = offline since the PR's updatedAt`, () => {
    const conflict = (updatedAt: string) => pr({ mergeState: `conflict`, mergeHold: null, updatedAt })
    const at = (updatedAt: string, list: never) =>
      withStuckRows([], { pullRequests: [conflict(updatedAt)], sessions, chats, devices: list, now, owner: true })
    // No devices row synced: never online, waits 2 h from the PR's last write.
    expect(at(`2027-01-15T09:00:00.000Z`, [] as never)).toEqual([])
    expect(at(`2027-01-15T08:00:00.000Z`, [] as never)[0]?.meta).toBe(`Device offline`)
    // A row without a heartbeat.
    expect(at(`2027-01-15T08:00:00.000Z`, devices(null))[0]?.meta).toBe(`Device offline`)
  })
})

// merge-train.json `offlineSince` over the synced rows.
describe(`prDeviceOfflineSince`, () => {
  const now = Date.parse(`2027-01-15T10:00:00.000Z`)
  const updatedAt = `2027-01-15T08:00:00.000Z`
  const sessions = new Map([
    [`r1`, { deviceId: `mac`, userId: `me` }],
    [`c1`, { deviceId: `pc`, userId: `me` }],
    [`r2`, { deviceId: ``, userId: `me` }],
  ])
  const device = (deviceId: string, lastSeenAt: string | null) => ({ deviceId, userId: `me`, label: deviceId, lastSeenAt })
  const since = (sessionId: string | null, rootId: string, list: unknown[]) =>
    prDeviceOfflineSince({ sessionId, updatedAt } as never, rootId, sessions, list as never, now)

  it(`the opener's device online = none; offline = its last heartbeat`, () => {
    expect(since(`c1`, `r1`, [device(`pc`, `2027-01-15T09:59:50.000Z`)])).toBeNull()
    expect(since(`c1`, `r1`, [device(`pc`, `2027-01-15T07:30:00.000Z`)])).toBe(`2027-01-15T07:30:00.000Z`)
  })

  it(`an unknown last-seen = the PR's updatedAt`, () => {
    expect(since(`c1`, `r1`, [device(`pc`, null)])).toBe(updatedAt)
    expect(since(`c1`, `r1`, [device(`mac`, `2027-01-15T09:59:50.000Z`)])).toBe(updatedAt)
  })

  it(`no sessionId session synced = the root; no device on the opener = none`, () => {
    expect(since(null, `r1`, [device(`mac`, `2027-01-15T07:00:00.000Z`)])).toBe(`2027-01-15T07:00:00.000Z`)
    expect(since(`gone`, `r1`, [])).toBe(updatedAt)
    expect(since(null, `r2`, [])).toBeNull()
    expect(since(null, `unknown`, [])).toBeNull()
  })
})

// #EXP-1710: an `approval` ready PR row's press = Needs approval.
describe(`withApprovals`, () => {
  const ready = {
    messageId: `p1`, sessionId: `r1`, rootId: `r1`, kind: `pr` as const, opens: `results` as const,
    title: `Ship`, chat: `Palette`, meta: `#12`, age: `3h`, createdAt: `2027-01-15T07:00:00.000Z`,
  }
  const pr = { id: `p1`, number: 12 } as never

  it(`approval mode: the ready row carries its PR; a stuck row never`, () => {
    expect(withApprovals([ready], [pr], `approval`)[0]?.approve).toBe(pr)
    const stuck = { ...ready, stuck: { pullRequest: pr, actions: [] } }
    expect(withApprovals([stuck], [pr], `approval`)[0]?.approve).toBeUndefined()
  })

  it(`auto mode: untouched`, () => {
    expect(withApprovals([ready], [pr], `auto`)[0]?.approve).toBeUndefined()
  })
})
