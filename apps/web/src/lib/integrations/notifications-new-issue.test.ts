import { beforeEach, describe, expect, it, vi } from "vitest"

// Locks the EXP-53 fan-out: fireAndForgetNewIssueNotify writes an
// `issue_created` notification to every human member of the issue's team
// (no actor to exclude — the widget bot creator is agent-filtered by
// deliver()), plus the EXP-50 guarantee that fireAndForgetAssignmentNotify
// self-filters when the (defaulted) assignee IS the actor.
//
// Also locks the EXP-264 push payload on BOTH fan-out paths: the shared data
// carries the deep-link parts (teamSlug/boardSlug) and every recipient gets
// the id of the notification row actually written for them, so a tapped push
// can route and mark-read without waiting for the Electric shapes.

const h = vi.hoisted(() => ({
  // Each db.select() call consumes the next result set, in call order.
  selectQueue: [] as unknown[][],
  executeRows: [] as Array<{ id: string; user_id: string }>,
  sendToUsers: vi.fn(async () => undefined),
}))

vi.mock(`@/db/connection`, () => {
  const builder = (result: unknown[]) => {
    const b = {
      from: () => b,
      innerJoin: () => b,
      where: () => b,
      orderBy: () => b,
      limit: async () => result,
      // Awaited without .limit() (the member/recipient enumerations).
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      then: (res: any, rej: any) => Promise.resolve(result).then(res, rej),
    }
    return b
  }
  return {
    db: {
      select: vi.fn(() => builder(h.selectQueue.shift() ?? [])),
      execute: vi.fn(async () => ({ rows: h.executeRows })),
      update: vi.fn(),
      insert: vi.fn(),
    },
  }
})

vi.mock(`@/lib/integrations/fcm`, () => ({
  sendToUsers: h.sendToUsers,
}))

vi.mock(`@/lib/email`, () => ({
  emailEnabled: false,
  sendReporterResolutionEmail: vi.fn(),
}))

import { db } from "@/db/connection"
import {
  fireAndForgetAssignmentNotify,
  fireAndForgetNewIssueNotify,
  fireAndForgetSupportThreadNotify,
} from "@/lib/integrations/notifications"

const issueMeta = {
  id: `33333333-3333-4333-8333-333333333333`,
  identifier: `EXP-7`,
  title: `Login button unresponsive`,
  teamId: `ws-1`,
  teamSlug: `acme`,
  boardSlug: `feedback`,
  assigneeId: null,
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const mockedDb = db as any

describe(`fireAndForgetNewIssueNotify (EXP-53)`, () => {
  beforeEach(() => {
    h.selectQueue.length = 0
    h.executeRows.length = 0
    h.sendToUsers.mockClear()
    mockedDb.select.mockClear()
    mockedDb.execute.mockClear()
  })

  it(`delivers issue_created to every human team member`, async () => {
    h.selectQueue.push(
      // loadIssueMeta
      [issueMeta],
      // team member enumeration (u3 is the widget bot's membership row)
      [{ userId: `u1` }, { userId: `u2` }, { userId: `u3` }],
      // deliverableRecipients: current non-agent members — drops the bot u3
      [{ id: `u1` }, { id: `u2` }]
    )
    h.executeRows.push(
      { id: `n1`, user_id: `u1` },
      { id: `n2`, user_id: `u2` }
    )

    fireAndForgetNewIssueNotify({ issueId: issueMeta.id })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))

    // The notification insert ran once (rows for u1+u2 came back from it).
    expect(mockedDb.execute).toHaveBeenCalledTimes(1)

    // ONE batched push call covering both delivered recipients (REV2-3), each
    // carrying the notification id RETURNING gave for that user (EXP-264).
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [
        { userId: `u1`, data: { notificationId: `n1` } },
        { userId: `u2`, data: { notificationId: `n2` } },
      ],
      {
        title: `New feedback: EXP-7`,
        body: `Login button unresponsive`,
        data: {
          type: `issue_created`,
          issueId: issueMeta.id,
          identifier: `EXP-7`,
          // The deep link a tap needs, without a synced issue row.
          teamSlug: `acme`,
          boardSlug: `feedback`,
        },
      }
    )
  })

  it(`does nothing when the team has no deliverable members`, async () => {
    h.selectQueue.push(
      [issueMeta],
      // Only the bot's membership row…
      [{ userId: `u3` }],
      // …which deliverableRecipients filters out.
      []
    )

    fireAndForgetNewIssueNotify({ issueId: issueMeta.id })

    // Drain the fire-and-forget chain, then confirm no insert / push.
    await vi.waitFor(() => expect(mockedDb.select).toHaveBeenCalledTimes(3))
    await Promise.resolve()
    expect(mockedDb.execute).not.toHaveBeenCalled()
    expect(h.sendToUsers).not.toHaveBeenCalled()
  })

  it(`does nothing when the issue is gone`, async () => {
    h.selectQueue.push([])

    fireAndForgetNewIssueNotify({ issueId: issueMeta.id })

    await vi.waitFor(() => expect(mockedDb.select).toHaveBeenCalledTimes(1))
    await Promise.resolve()
    expect(mockedDb.execute).not.toHaveBeenCalled()
    expect(h.sendToUsers).not.toHaveBeenCalled()
  })
})

describe(`issue-less support fan-out push payload (EXP-264)`, () => {
  beforeEach(() => {
    h.selectQueue.length = 0
    h.executeRows.length = 0
    h.sendToUsers.mockClear()
    mockedDb.select.mockClear()
    mockedDb.execute.mockClear()
  })

  it(`carries the thread id and each recipient's own notification id`, async () => {
    h.selectQueue.push(
      // the thread
      [
        {
          id: `t-1`,
          teamId: `ws-1`,
          title: `Cannot log in`,
          reporterName: `Ada`,
          reporterEmail: `ada@example.com`,
        },
      ],
      // team member enumeration
      [{ userId: `u1` }, { userId: `u2` }],
      // latest inbound public message (the preview)
      [{ body: `Still broken after a reload` }],
      // deliverableRecipients: current members
      [{ id: `u1` }, { id: `u2` }]
    )
    h.executeRows.push(
      { id: `n1`, user_id: `u1` },
      { id: `n2`, user_id: `u2` }
    )

    fireAndForgetSupportThreadNotify({ threadId: `t-1`, kind: `reply` })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))

    // These rows have no issue, so `threadId` is the whole routing key —
    // paired per recipient with the notification row written for them.
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [
        { userId: `u1`, data: { notificationId: `n1` } },
        { userId: `u2`, data: { notificationId: `n2` } },
      ],
      {
        title: `Ada replied on a support ticket`,
        body: `Still broken after a reload`,
        data: { type: `support_reply`, threadId: `t-1` },
      }
    )
  })
})

describe(`fireAndForgetAssignmentNotify self-filter (EXP-50 guarantee)`, () => {
  beforeEach(() => {
    h.selectQueue.length = 0
    h.sendToUsers.mockClear()
    mockedDb.select.mockClear()
    mockedDb.execute.mockClear()
  })

  it(`skips entirely when the new assignee IS the actor`, async () => {
    fireAndForgetAssignmentNotify({
      issueId: issueMeta.id,
      actorUserId: `solo`,
      newAssigneeId: `solo`,
    })

    // Synchronous early-return: no db reads, no insert, no push — a solo
    // team defaulting the assignee to the creator never self-notifies.
    await Promise.resolve()
    expect(mockedDb.select).not.toHaveBeenCalled()
    expect(mockedDb.execute).not.toHaveBeenCalled()
    expect(h.sendToUsers).not.toHaveBeenCalled()
  })
})
