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
  for (const m of [`from`, `where`, `innerJoin`, `leftJoin`, `limit`]) {
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

  it(`names an issue run by its identifier and pushes the session target`, async () => {
    selectResults.push([
      {
        userId: `owner`,
        teamId: `team-1`,
        teamSlug: `acme`,
        issueId: `issue-1`,
        batchIssueIds: null,
        actionName: null,
      },
    ])
    selectResults.push([{ identifier: `EXP-12` }])
    selectResults.push([{ id: `owner` }]) // membership guard
    executeState.rows = [{ id: `n-1`, user_id: `owner` }]
    selectResults.push([]) // push prefs: not muted

    await notifySessionBlocked(`sess-1`, blocked)

    expect(executeState.calls).toBe(1)
    const [recipients, payload] = vi.mocked(sendToUsers).mock.calls[0]!
    expect(recipients).toEqual([
      { userId: `owner`, data: { notificationId: `n-1` } },
    ])
    expect(payload).toMatchObject({
      title: `EXP-12 hit a rate limit`,
      body: `Rate limited`,
      data: {
        type: `session_blocked`,
        sessionId: `sess-1`,
        teamId: `team-1`,
        teamSlug: `acme`,
      },
    })
  })

  it(`names a batch by its first issue and the rest, an action run by its name`, async () => {
    selectResults.push([
      {
        userId: `owner`,
        teamId: `team-1`,
        teamSlug: `acme`,
        issueId: null,
        batchIssueIds: [`issue-1`, `issue-2`, `issue-3`],
        actionName: null,
      },
    ])
    // The covered rows, in whatever order the lookup returns them: the name
    // follows the STORED order (lib/batch-run.ts), the same rule every list
    // row uses, so the push and the row it opens agree.
    selectResults.push([
      { id: `issue-3`, identifier: `EXP-14`, title: `Third` },
      { id: `issue-1`, identifier: `EXP-12`, title: `First` },
      { id: `issue-2`, identifier: `EXP-13`, title: `Second` },
    ])
    selectResults.push([{ id: `owner` }])
    executeState.rows = [{ id: `n-1`, user_id: `owner` }]
    selectResults.push([])
    await notifySessionBlocked(`sess-1`, blocked)
    expect(vi.mocked(sendToUsers).mock.calls[0]![1].title).toBe(
      `EXP-12 +2 hit a rate limit`
    )

    // A lead issue that no longer resolves: the next covered one leads and
    // the stored count still names the whole batch — batchRunName's rule,
    // not a fall-through to `Agent run`.
    vi.mocked(sendToUsers).mockClear()
    selectResults.push([
      {
        userId: `owner`,
        teamId: `team-1`,
        teamSlug: `acme`,
        issueId: null,
        batchIssueIds: [`gone`, `issue-2`, `issue-3`],
        actionName: null,
      },
    ])
    selectResults.push([
      { id: `issue-2`, identifier: `EXP-13`, title: `Second` },
      { id: `issue-3`, identifier: `EXP-14`, title: `Third` },
    ])
    selectResults.push([{ id: `owner` }])
    executeState.rows = [{ id: `n-1`, user_id: `owner` }]
    selectResults.push([])
    await notifySessionBlocked(`sess-1`, blocked)
    expect(vi.mocked(sendToUsers).mock.calls[0]![1].title).toBe(
      `EXP-13 +2 hit a rate limit`
    )

    vi.mocked(sendToUsers).mockClear()
    selectResults.push([
      {
        userId: `owner`,
        teamId: `team-1`,
        teamSlug: `acme`,
        issueId: null,
        batchIssueIds: null,
        actionName: `Triage inbox`,
      },
    ])
    selectResults.push([{ id: `owner` }])
    selectResults.push([])
    await notifySessionBlocked(`sess-2`, blocked)
    expect(vi.mocked(sendToUsers).mock.calls[0]![1].title).toBe(
      `Triage inbox hit a rate limit`
    )
  })

  it(`writes nothing for an owner who left the team, and never throws`, async () => {
    selectResults.push([
      {
        userId: `owner`,
        teamId: `team-1`,
        teamSlug: `acme`,
        issueId: null,
        batchIssueIds: null,
        actionName: null,
      },
    ])
    selectResults.push([]) // membership guard: gone
    await notifySessionBlocked(`sess-1`, blocked)
    expect(executeState.calls).toBe(0)
    expect(sendToUsers).not.toHaveBeenCalled()

    // A vanished run row is a no-op too.
    selectResults.push([])
    await expect(notifySessionBlocked(`sess-9`, blocked)).resolves.toBeUndefined()
  })
})
