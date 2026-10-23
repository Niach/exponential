import { beforeEach, describe, expect, it, vi } from "vitest"

// deliverableRecipients() is the membership guard at deliver()'s chokepoint
// (REV-8): stale issue_subscribers rows / assignee ids left behind by a
// removed member must never fan out to inbox/push/email. The db is mocked
// billing.test.ts-style — `db.select()` shifts the next pre-seeded result
// array off a FIFO queue, so the joined membership query can be scripted
// without Postgres.
const { selectResults, selectCalls, executeState } = vi.hoisted(() => ({
  selectResults: [] as unknown[][],
  selectCalls: { count: 0 },
  // EXP-801: the insert…returning statement sendAgentMessage runs.
  executeState: { rows: [] as unknown[], calls: 0 },
}))

function chain(): Promise<unknown[]> & Record<string, () => unknown> {
  const p = Promise.resolve(
    selectResults.shift() ?? []
  ) as Promise<unknown[]> & Record<string, () => unknown>
  for (const m of [`from`, `where`, `innerJoin`, `leftJoin`, `limit`, `orderBy`]) {
    p[m] = () => p
  }
  return p
}

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => {
      selectCalls.count += 1
      return chain()
    },
    execute: async () => {
      executeState.calls += 1
      return { rows: executeState.rows }
    },
  },
}))

// Importing the module under test must touch neither FCM nor the email
// transport.
vi.mock(`@/lib/integrations/fcm`, () => ({
  sendToUsers: vi.fn(async () => {}),
}))
vi.mock(`@/lib/email`, () => ({
  emailEnabled: false,
  sendReporterResolutionEmail: vi.fn(),
}))

vi.mock(`@/lib/metrics/registry`, () => ({
  recordNotificationFanout: vi.fn(),
}))

import { sendToUsers } from "@/lib/integrations/fcm"
import {
  deliverableRecipients,
  notifySessionBlocked,
  sendAgentMessage,
} from "./notifications"

beforeEach(() => {
  selectResults.length = 0
  selectCalls.count = 0
  executeState.rows = []
  executeState.calls = 0
  vi.mocked(sendToUsers).mockClear()
})

describe(`deliverableRecipients — membership guard at the deliver() chokepoint`, () => {
  it(`drops recipients who are not current team members, preserving input order`, async () => {
    // The team_members ⋈ users query returns only the current,
    // non-agent members among the candidates — `removed-b` (an ex-member
    // with a stale subscriber row) is absent.
    selectResults.push([{ id: `member-a` }, { id: `member-c` }])

    const result = await deliverableRecipients(`ws-1`, [
      `member-c`,
      `removed-b`,
      `member-a`,
    ])

    expect(result).toEqual([`member-c`, `member-a`])
    expect(selectCalls.count).toBe(1)
  })

  it(`short-circuits an empty recipient list without querying the db`, async () => {
    const result = await deliverableRecipients(`ws-1`, [])

    expect(result).toEqual([])
    expect(selectCalls.count).toBe(0)
  })
})

// EXP-801: the agent-message send answers synchronously with one bucket per
// recipient. Select order inside sendAgentMessage: team slug → blocklist →
// sender name → membership guard → (after the insert) per-type push prefs.
describe(`sendAgentMessage — blocked recipients never get a row, self always passes`, () => {
  it(`buckets declined, non-member, deduped and delivered recipients`, async () => {
    selectResults.push([{ slug: `acme` }])
    // `blocked` turned teammates' agents off; the sender is never looked up.
    selectResults.push([{ userId: `blocked` }])
    selectResults.push([{ name: `Ada`, email: `ada@example.com` }])
    // Membership guard: `gone` is no longer a member.
    selectResults.push([{ id: `sender` }, { id: `peer` }, { id: `dup` }])
    // The insert wrote rows for sender + peer; `dup` hit the dedupe window.
    executeState.rows = [
      { id: `n-1`, user_id: `sender` },
      { id: `n-2`, user_id: `peer` },
    ]
    // Push prefs: nobody muted the type.
    selectResults.push([])

    const outcome = await sendAgentMessage({
      teamId: `team-1`,
      senderUserId: `sender`,
      recipientIds: [`sender`, `peer`, `blocked`, `gone`, `dup`, `peer`],
      title: `Build finished`,
      body: `All green.`,
    })

    expect(outcome).toEqual({
      delivered: [`sender`, `peer`],
      declined: [`blocked`],
      notMembers: [`gone`],
      deduped: [`dup`],
    })
    expect(executeState.calls).toBe(1)
    // Push-first like every other fan-out; the payload carries the team, no
    // issue keys, and the sender's name in the sentence.
    expect(sendToUsers).toHaveBeenCalledTimes(1)
    const [recipients, payload] = vi.mocked(sendToUsers).mock.calls[0]!
    expect(recipients.map((r) => r.userId)).toEqual([`sender`, `peer`])
    expect(payload).toMatchObject({
      title: `Ada's agent: Build finished`,
      body: `All green.`,
      data: { type: `agent_message`, teamId: `team-1`, teamSlug: `acme` },
    })
  })

  it(`skips the insert entirely when every recipient declined`, async () => {
    selectResults.push([{ slug: `acme` }])
    selectResults.push([{ userId: `blocked` }])
    selectResults.push([{ name: `Ada`, email: `ada@example.com` }])
    // Membership guard short-circuits on an empty list — no query.

    const outcome = await sendAgentMessage({
      teamId: `team-1`,
      senderUserId: `sender`,
      recipientIds: [`blocked`],
      title: `Ping`,
      body: null,
    })

    expect(outcome).toEqual({
      delivered: [],
      declined: [`blocked`],
      notMembers: [],
      deduped: [],
    })
    expect(executeState.calls).toBe(0)
    expect(sendToUsers).not.toHaveBeenCalled()
  })
})

// EXP-980: a walled run tells its OWNER — every run, not only a child's
// parent agent.
describe(`notifySessionBlocked — the run's owner gets a row and a push that route to the run`, () => {
  const blocked = {
    kind: `rate_limit`,
    agent: `claude`,
    window: `weekly`,
    resetsAt: null,
    since: `2026-09-19T00:00:00Z`,
  } as const

  const session = {
    id: `sess-1`,
    rootId: `sess-1`,
    title: null,
    actionName: null,
    name: null,
    userId: `owner`,
    teamId: `team-1`,
    teamSlug: `acme`,
    deviceId: `dev-1`,
    cli: `claude`,
    account: null,
  }

  it(`names the session by its title and pushes the session target`, async () => {
    selectResults.push([{ ...session, title: `Fix login` }])
    selectResults.push([]) // throttle: no recent wall
    // EXP-1341: a stored title names it before any lookup.
    selectResults.push([{ id: `owner` }]) // membership guard
    executeState.rows = [{ id: `n-1`, user_id: `owner` }]
    selectResults.push([]) // push prefs: not muted

    await notifySessionBlocked(`sess-1`, blocked)

    expect(executeState.calls).toBe(1)
    const [recipients, payload] = vi.mocked(sendToUsers).mock.calls[0]!
    expect(recipients).toEqual([{ userId: `owner`, data: { notificationId: `n-1` } }])
    expect(payload).toMatchObject({
      title: `Fix login hit a rate limit`,
      body: `Rate limited`,
      data: { type: `session_blocked`, sessionId: `sess-1`, teamId: `team-1`, teamSlug: `acme` },
    })
  })

  it(`derives an untitled thread's name from its first issue, else Untitled`, async () => {
    selectResults.push([session])
    selectResults.push([]) // throttle
    // EXP-1341 session-title.json: the tree's links, then the input messages.
    selectResults.push([
      {
        sessionId: `sess-1`,
        issueId: `i-12`,
        sortOrder: 1,
        assignedSessionId: null,
        id: `i-12`,
        identifier: `EXP-12`,
        title: `Login bug`,
      },
    ])
    selectResults.push([])
    selectResults.push([{ id: `owner` }])
    executeState.rows = [{ id: `n-1`, user_id: `owner` }]
    selectResults.push([])
    await notifySessionBlocked(`sess-1`, blocked)
    expect(vi.mocked(sendToUsers).mock.calls[0]![1].title).toBe(`#EXP-12 Login bug hit a rate limit`)

    vi.mocked(sendToUsers).mockClear()
    selectResults.push([session])
    selectResults.push([])
    selectResults.push([]) // links: none
    selectResults.push([]) // messages: none
    selectResults.push([{ id: `owner` }])
    selectResults.push([])
    await notifySessionBlocked(`sess-2`, blocked)
    expect(vi.mocked(sendToUsers).mock.calls[0]![1].title).toBe(`Untitled hit a rate limit`)
  })

  it(`writes nothing for an owner who left the team, and never throws`, async () => {
    selectResults.push([{ ...session, title: `Gone` }])
    selectResults.push([]) // throttle: no recent wall
    selectResults.push([]) // membership guard: gone
    await notifySessionBlocked(`sess-1`, blocked)
    expect(executeState.calls).toBe(0)
    expect(sendToUsers).not.toHaveBeenCalled()

    // A vanished run row is a no-op too.
    selectResults.push([])
    await expect(notifySessionBlocked(`sess-9`, blocked)).resolves.toBeUndefined()
  })

  // EXP-1005: at most one wall notification per PROFILE (device × agent ×
  // account) per owner per hour. The throttle query returns the owner's
  // recent session_blocked rows with their runs' profiles.
  describe(`the per-profile hourly throttle`, () => {
    const walled = { ...session, actionName: `Triage inbox` }
    const minutesAgo = (m: number) => new Date(Date.now() - m * 60_000)

    function expectDelivered() {
      selectResults.push([]) // title: links
      selectResults.push([]) // title: input messages
      selectResults.push([{ id: `owner` }]) // membership guard
      executeState.rows = [{ id: `n-2`, user_id: `owner` }]
      selectResults.push([]) // push prefs
    }

    it(`a second wall on the same profile within the hour is not delivered`, async () => {
      selectResults.push([walled])
      selectResults.push([
        {
          createdAt: minutesAgo(20),
          deviceId: `dev-1`,
          cli: `claude`,
          account: null,
        },
      ])
      await notifySessionBlocked(`sess-2`, blocked)
      expect(executeState.calls).toBe(0)
      expect(sendToUsers).not.toHaveBeenCalled()
    })

    it(`a wall on another profile of the same device is delivered`, async () => {
      selectResults.push([{ ...walled, account: `work` }])
      selectResults.push([
        {
          createdAt: minutesAgo(5),
          deviceId: `dev-1`,
          cli: `claude`,
          account: null, // an unrecorded account
        },
        {
          createdAt: minutesAgo(5),
          deviceId: `dev-1`,
          cli: `codex`,
          account: `work`,
        },
      ])
      expectDelivered()
      await notifySessionBlocked(`sess-2`, blocked)
      expect(executeState.calls).toBe(1)
      expect(sendToUsers).toHaveBeenCalledTimes(1)
    })

    it(`an older wall past the hour is delivered again`, async () => {
      selectResults.push([walled])
      selectResults.push([
        {
          createdAt: minutesAgo(61),
          deviceId: `dev-1`,
          cli: `claude`,
          account: null,
        },
      ])
      expectDelivered()
      await notifySessionBlocked(`sess-2`, blocked)
      expect(executeState.calls).toBe(1)
      expect(sendToUsers).toHaveBeenCalledTimes(1)
    })
  })
})
