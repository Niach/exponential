import { beforeEach, describe, expect, it, vi } from "vitest"

// Locks the EXP-463 PR fan-out attribution: fireAndForgetPrNotify excludes
// the actor from the recipient set, and when the caller has no actor (the
// GitHub webhook / the self-hosted merge poller) it falls back to the owner
// of the issue's most recent coding session — the human whose agent produced
// the PR. The fallback title matches the attributed MCP-tool title exactly,
// so deliver()'s dedupe window collapses the webhook-vs-open_pr racing pair.
// A PR with no session stays anonymous with nobody excluded (locked here so
// a fallback regression can't silently drop notifications).

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
      // Awaited without .limit() (the subscriber/recipient enumerations).
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
import { fireAndForgetPrNotify } from "@/lib/integrations/notifications"

const issueMeta = {
  id: `44444444-4444-4444-8444-444444444444`,
  identifier: `EXP-9`,
  title: `Fix the widget capture crash`,
  teamId: `ws-1`,
  teamSlug: `acme`,
  boardSlug: `bugs`,
  // Null keeps the assignee opt-out lookup out of the select order — the
  // assignee-add path is exercised by the recipients themselves here.
  assigneeId: null,
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const mockedDb = db as any

describe(`fireAndForgetPrNotify actor attribution (EXP-463)`, () => {
  beforeEach(() => {
    h.selectQueue.length = 0
    h.executeRows.length = 0
    h.sendToUsers.mockClear()
    mockedDb.select.mockClear()
    mockedDb.execute.mockClear()
  })

  it(`excludes an explicit actor and names them in the title`, async () => {
    h.selectQueue.push(
      // loadIssueMeta
      [issueMeta],
      // subscriberRecipients: the actor is subscribed too
      [{ userId: `u-actor` }, { userId: `u2` }],
      // actorName
      [{ name: `Ada`, email: `ada@acme.test` }],
      // deliverableRecipients (deliver)
      [{ id: `u2` }]
    )
    h.executeRows.push({ id: `n1`, user_id: `u2` })

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_opened`,
      actorUserId: `u-actor`,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [{ userId: `u2`, data: { notificationId: `n1` } }],
      expect.objectContaining({
        title: `Ada opened a pull request for EXP-9`,
      })
    )
  })

  it(`falls back to the coding-session owner when the actor is null`, async () => {
    h.selectQueue.push(
      // loadIssueMeta
      [issueMeta],
      // sessionOwnerFallback: the issue's most recent coding session
      [{ userId: `u-owner` }],
      // subscriberRecipients: the owner is the auto-subscribed creator
      [{ userId: `u-owner` }, { userId: `u2` }],
      // actorName (resolved for the fallback owner)
      [{ name: `Danny`, email: `danny@acme.test` }],
      // deliverableRecipients (deliver)
      [{ id: `u2` }]
    )
    h.executeRows.push({ id: `n2`, user_id: `u2` })

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_merged`,
      actorUserId: null,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    // The owner is excluded, and the title is byte-identical to the one the
    // attributed in-app path produces — the dedupe-collapse contract.
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [{ userId: `u2`, data: { notificationId: `n2` } }],
      expect.objectContaining({
        title: `Danny merged the pull request for EXP-9`,
      })
    )
  })

  it(`stays anonymous with nobody excluded when no session exists`, async () => {
    h.selectQueue.push(
      // loadIssueMeta
      [issueMeta],
      // sessionOwnerFallback: no coding session for this issue
      [],
      // subscriberRecipients
      [{ userId: `u1` }, { userId: `u2` }],
      // deliverableRecipients (no actorName lookup — actor stayed null)
      [{ id: `u1` }, { id: `u2` }]
    )
    h.executeRows.push(
      { id: `n3`, user_id: `u1` },
      { id: `n4`, user_id: `u2` }
    )

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_opened`,
      actorUserId: null,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [
        { userId: `u1`, data: { notificationId: `n3` } },
        { userId: `u2`, data: { notificationId: `n4` } },
      ],
      expect.objectContaining({
        title: `A pull request was opened for EXP-9`,
      })
    )
  })
})
