import { beforeEach, describe, expect, it, vi } from "vitest"

// Locks the EXP-463 PR fan-out attribution: fireAndForgetPrNotify excludes
// the actor from the recipient set, and when the caller has no actor (the
// GitHub webhook / the self-hosted merge poller) it falls back to the owner
// of the issue's most recent coding session — the human whose agent produced
// the PR. The fallback title matches the attributed MCP-tool title exactly,
// so deliver()'s dedupe window collapses the webhook-vs-open_pr racing pair.
// A PR with no session stays anonymous with nobody excluded (locked here so
// a fallback regression can't silently drop notifications).
//
// EXP-479: batch runs are issue-less, so an issue linked to a batch PR (its
// branch is the launcher's `exp/batch-<id8>` convention) resolves through the
// team's most recent batch-shaped session instead; non-batch branches never
// take that lookup, keeping out-of-band PRs anonymous.

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
  // Non-batch by default: the batch-session lookup must consume a select ONLY
  // for `exp/batch-` branches (the queues below depend on it).
  branch: null,
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

  it(`resolves a batch PR to the team's batch-session owner (EXP-479)`, async () => {
    h.selectQueue.push(
      // loadIssueMeta: the issue is linked to a batch run's combined PR
      [{ ...issueMeta, branch: `exp/batch-1b81d4c7` }],
      // sessionOwnerFallback: no issue-scoped session (batch rows are
      // issue-less)…
      [],
      // …so the team-scoped batch lookup resolves the owner
      [{ userId: `u-owner` }],
      // subscriberRecipients: the owner is the auto-subscribed creator
      [{ userId: `u-owner` }, { userId: `u2` }],
      // actorName (resolved for the batch owner)
      [{ name: `Danny`, email: `danny@acme.test` }],
      // deliverableRecipients (deliver)
      [{ id: `u2` }]
    )
    h.executeRows.push({ id: `n5`, user_id: `u2` })

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_merged`,
      actorUserId: null,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    // The batch owner is excluded from their own agent's fan-out, and the
    // title matches the attributed MCP variant — the dedupe-collapse contract.
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [{ userId: `u2`, data: { notificationId: `n5` } }],
      expect.objectContaining({
        title: `Danny merged the pull request for EXP-9`,
      })
    )
  })

  // EXP-494: an actor resolved from an agent's MCP credential may be only a
  // proxy identity — on a shared CLI server the daemon holds its OWNER's key
  // while the session row is requester-owned (EXP-432). With actorViaAgent
  // the fan-out swaps host→requester so exclusion and title match what the
  // webhook leg's fallback resolves; without it (a real human acting from
  // web/mobile) the actor is taken at face value and NO session lookup runs.
  it(`swaps a viaAgent actor to the session requester when the actor is its host`, async () => {
    h.selectQueue.push(
      // loadIssueMeta
      [issueMeta],
      // latestSessionForIssue: requester-owned row hosted by the actor
      [{ userId: `u-req`, hostUserId: `u-host` }],
      // subscriberRecipients: the requester is the auto-subscribed creator
      [{ userId: `u-req` }, { userId: `u2` }],
      // actorName (resolved for the REQUESTER)
      [{ name: `Riley`, email: `riley@acme.test` }],
      // deliverableRecipients (deliver)
      [{ id: `u2` }]
    )
    h.executeRows.push({ id: `n8`, user_id: `u2` })

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_opened`,
      actorUserId: `u-host`,
      actorViaAgent: true,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [{ userId: `u2`, data: { notificationId: `n8` } }],
      expect.objectContaining({
        title: `Riley opened a pull request for EXP-9`,
      })
    )
  })

  it(`keeps a viaAgent actor when no session exists (the claim-attributed webhook leg)`, async () => {
    h.selectQueue.push(
      // loadIssueMeta
      [issueMeta],
      // latestSessionForIssue: nothing (sessionless CLI run / swept row)
      [],
      // subscriberRecipients: the actor is the auto-subscribed creator
      [{ userId: `u-actor` }, { userId: `u2` }],
      // actorName
      [{ name: `Ada`, email: `ada@acme.test` }],
      // deliverableRecipients (deliver)
      [{ id: `u2` }]
    )
    h.executeRows.push({ id: `n9`, user_id: `u2` })

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_opened`,
      actorUserId: `u-actor`,
      actorViaAgent: true,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    // The actor stays excluded and named — the reported EXP-494 bug was this
    // exact leg firing anonymously when the session row was gone.
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [{ userId: `u2`, data: { notificationId: `n9` } }],
      expect.objectContaining({
        title: `Ada opened a pull request for EXP-9`,
      })
    )
  })

  it(`keeps a viaAgent actor when the session is not hosted by them`, async () => {
    h.selectQueue.push(
      // loadIssueMeta
      [issueMeta],
      // latestSessionForIssue: a self-started row (hostUserId null)
      [{ userId: `u-owner`, hostUserId: null }],
      // subscriberRecipients
      [{ userId: `u-actor` }, { userId: `u2` }],
      // actorName (still the actor — no swap)
      [{ name: `Ada`, email: `ada@acme.test` }],
      // deliverableRecipients (deliver)
      [{ id: `u2` }]
    )
    h.executeRows.push({ id: `n10`, user_id: `u2` })

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_merged`,
      actorUserId: `u-actor`,
      actorViaAgent: true,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [{ userId: `u2`, data: { notificationId: `n10` } }],
      expect.objectContaining({
        title: `Ada merged the pull request for EXP-9`,
      })
    )
  })

  it(`never swaps (or even looks up sessions) for a human actor`, async () => {
    h.selectQueue.push(
      // loadIssueMeta — NO session select follows: a host merging a
      // requester's PR from the web UI is a genuine human action and the
      // requester must be notified about it.
      [issueMeta],
      // subscriberRecipients: the requester is subscribed
      [{ userId: `u-req` }, { userId: `u2` }],
      // actorName (the host, unswapped)
      [{ name: `Harper`, email: `harper@acme.test` }],
      // deliverableRecipients (deliver)
      [{ id: `u-req` }, { id: `u2` }]
    )
    h.executeRows.push(
      { id: `n11`, user_id: `u-req` },
      { id: `n12`, user_id: `u2` }
    )

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_merged`,
      actorUserId: `u-host`,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    // Exactly the four selects above plus pushRecipients' prefs lookup — no
    // latestSessionForIssue call (a viaAgent run would add a fifth queue
    // select before actorName).
    expect(mockedDb.select).toHaveBeenCalledTimes(5)
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [
        { userId: `u-req`, data: { notificationId: `n11` } },
        { userId: `u2`, data: { notificationId: `n12` } },
      ],
      expect.objectContaining({
        title: `Harper merged the pull request for EXP-9`,
      })
    )
  })

  it(`swaps a viaAgent actor through the batch-session fallback (EXP-479 shape)`, async () => {
    h.selectQueue.push(
      // loadIssueMeta: batch PR branch
      [{ ...issueMeta, branch: `exp/batch-1b81d4c7` }],
      // latestSessionForIssue: no issue-scoped row (batch rows are issue-less)…
      [],
      // …the team-scoped batch row is requester-owned, hosted by the actor
      [{ userId: `u-req`, hostUserId: `u-host` }],
      // subscriberRecipients
      [{ userId: `u-req` }, { userId: `u2` }],
      // actorName (the requester)
      [{ name: `Riley`, email: `riley@acme.test` }],
      // deliverableRecipients (deliver)
      [{ id: `u2` }]
    )
    h.executeRows.push({ id: `n13`, user_id: `u2` })

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_merged`,
      actorUserId: `u-host`,
      actorViaAgent: true,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [{ userId: `u2`, data: { notificationId: `n13` } }],
      expect.objectContaining({
        title: `Riley merged the pull request for EXP-9`,
      })
    )
  })

  it(`keeps a sessionless batch PR anonymous`, async () => {
    h.selectQueue.push(
      // loadIssueMeta
      [{ ...issueMeta, branch: `exp/batch-1b81d4c7` }],
      // sessionOwnerFallback: no issue-scoped session…
      [],
      // …and no batch-shaped session in the team either
      [],
      // subscriberRecipients
      [{ userId: `u1` }, { userId: `u2` }],
      // deliverableRecipients (no actorName lookup — actor stayed null)
      [{ id: `u1` }, { id: `u2` }]
    )
    h.executeRows.push(
      { id: `n6`, user_id: `u1` },
      { id: `n7`, user_id: `u2` }
    )

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_merged`,
      actorUserId: null,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [
        { userId: `u1`, data: { notificationId: `n6` } },
        { userId: `u2`, data: { notificationId: `n7` } },
      ],
      expect.objectContaining({
        title: `The pull request for EXP-9 was merged`,
      })
    )
  })
})
