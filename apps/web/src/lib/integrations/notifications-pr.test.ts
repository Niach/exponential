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
//
// EXP-617 splits naming from excluding: the title still takes ONE actor, but
// recipients are filtered by a SET that also carries the agent-activity
// record, the mapped GitHub actor, and every consulted session's host. The
// cases at the bottom lock that a title can stay anonymous while the people
// behind the PR are still kept out of it — the old code could only do both or
// neither, which is what made the reported bug possible.

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
import {
  _clearPrActorClaims,
  noteAgentIssueActivity,
} from "@/lib/integrations/pr-actor-claims"

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
    _clearPrActorClaims()
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
      // loadPrSessionCandidates: the issue's most recent coding session
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
      // loadPrSessionCandidates: no coding session for this issue
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
      // loadPrSessionCandidates: no issue-scoped row (batch rows are
      // issue-less), so the team-scoped batch arm of the same query is what
      // resolves the owner
      [{ userId: `u-owner`, hostUserId: null }],
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
      // loadPrSessionCandidates: requester-owned row hosted by the actor
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
      // loadPrSessionCandidates: nothing (sessionless CLI run / swept row)
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
      // loadPrSessionCandidates: a self-started row (hostUserId null)
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
    // loadPrSessionCandidates call (a viaAgent run would add a fifth queue
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
      // loadPrSessionCandidates: the team-scoped batch row is requester-owned,
      // hosted by the actor
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
      // loadPrSessionCandidates: neither arm matches
      [],
      // subscriberRecipients
      [{ userId: `u1` }, { userId: `u2` }],
      // deliverableRecipients (no actorName lookup — actor stayed null)
      [{ id: `u1` }, { id: `u2` }]
    )
    h.executeRows.push({ id: `n6`, user_id: `u1` }, { id: `n7`, user_id: `u2` })

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

  // EXP-617's actual incident, reproduced from the real data: PR #507 on
  // EXP-616 was opened by the reporter's OWN GitHub account on github.com
  // (`pull_request.user` = a human, not the App bot), three seconds before the
  // notification landed. No claim, no coding session, nothing server-side to
  // attribute — the mapped GitHub identity is the only thing that can name
  // them, and it must keep them out of their own fan-out even though they are
  // the issue's creator AND assignee.
  it(`keeps the github.com PR author out of their own fan-out (EXP-616)`, async () => {
    h.selectQueue.push(
      // loadIssueMeta: creator and assignee are the same human who opened it
      [{ ...issueMeta, assigneeId: `u-author`, branch: `exp/EXP-9` }],
      // deliverableRecipients: the membership gate on the GitHub actor
      [{ id: `u-author` }],
      // subscriberRecipients: auto-subscribed as creator
      [{ userId: `u-author` }, { userId: `u2` }],
      // actorName
      [{ name: `Danny Strähhuber`, email: `danny@acme.test` }],
      // deliverableRecipients (deliver)
      [{ id: `u2` }]
    )
    h.executeRows.push({ id: `n21`, user_id: `u2` })

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_opened`,
      actorUserId: null,
      githubActorUserId: `u-author`,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [{ userId: `u2`, data: { notificationId: `n21` } }],
      expect.objectContaining({
        title: `Danny Strähhuber opened a pull request for EXP-9`,
      })
    )
  })

  // The second line of defence, for a PR author whose GitHub account has never
  // been connected here: an agent that filed the issue under someone's MCP
  // credential means notifying that person is telling them what they know.
  it(`keeps an agent-activity actor out of an anonymous fan-out`, async () => {
    noteAgentIssueActivity(issueMeta.id, `u-agent`)
    h.selectQueue.push(
      // loadIssueMeta
      [issueMeta],
      // loadPrSessionCandidates: nothing — the session belongs to another issue
      [],
      // subscriberRecipients: the agent's human is the auto-subscribed creator
      [{ userId: `u-agent` }, { userId: `u2` }],
      // deliverableRecipients — no actorName lookup, the title stays anonymous
      [{ id: `u2` }]
    )
    h.executeRows.push({ id: `n14`, user_id: `u2` })

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_opened`,
      actorUserId: null,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [{ userId: `u2`, data: { notificationId: `n14` } }],
      expect.objectContaining({
        title: `A pull request was opened for EXP-9`,
      })
    )
  })

  it(`only excludes agent activity on the issue it was recorded against`, async () => {
    noteAgentIssueActivity(`some-other-issue`, `u-agent`)
    h.selectQueue.push(
      // loadIssueMeta
      [issueMeta],
      // loadPrSessionCandidates
      [],
      // subscriberRecipients
      [{ userId: `u-agent` }],
      // deliverableRecipients
      [{ id: `u-agent` }]
    )
    h.executeRows.push({ id: `n15`, user_id: `u-agent` })

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_opened`,
      actorUserId: null,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [{ userId: `u-agent`, data: { notificationId: `n15` } }],
      expect.anything()
    )
  })

  // The session's HOST used to escape exclusion unless it happened to win the
  // host→requester swap: a shared CLI server's owner got pushed about a run
  // executing on their own machine.
  it(`excludes a consulted session's host as well as its owner`, async () => {
    h.selectQueue.push(
      // loadIssueMeta
      [issueMeta],
      // loadPrSessionCandidates: requester-owned, hosted elsewhere
      [{ userId: `u-req`, hostUserId: `u-host` }],
      // subscriberRecipients: both are subscribed
      [{ userId: `u-req` }, { userId: `u-host` }, { userId: `u2` }],
      // actorName (the session owner)
      [{ name: `Riley`, email: `riley@acme.test` }],
      // deliverableRecipients (deliver)
      [{ id: `u2` }]
    )
    h.executeRows.push({ id: `n16`, user_id: `u2` })

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_opened`,
      actorUserId: null,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [{ userId: `u2`, data: { notificationId: `n16` } }],
      expect.objectContaining({
        title: `Riley opened a pull request for EXP-9`,
      })
    )
  })

  // The merge-side answer: `merged_by` on github.com resolves to an app user,
  // who is named INSTEAD of the session owner (that used to be credited for a
  // merge they had nothing to do with) and kept out of their own fan-out.
  it(`names and excludes a mapped GitHub actor over the session fallback`, async () => {
    h.selectQueue.push(
      // loadIssueMeta
      [issueMeta],
      // deliverableRecipients: the membership gate on the GitHub actor
      [{ id: `u-gh` }],
      // subscriberRecipients — no session lookup: identity outranks it
      [{ userId: `u-gh` }, { userId: `u-owner` }, { userId: `u2` }],
      // actorName
      [{ name: `Harper`, email: `harper@acme.test` }],
      // deliverableRecipients (deliver)
      [{ id: `u-owner` }, { id: `u2` }]
    )
    h.executeRows.push(
      { id: `n17`, user_id: `u-owner` },
      { id: `n18`, user_id: `u2` }
    )

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_merged`,
      actorUserId: null,
      githubActorUserId: `u-gh`,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    // The session owner IS notified here — their teammate merged their work.
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [
        { userId: `u-owner`, data: { notificationId: `n17` } },
        { userId: `u2`, data: { notificationId: `n18` } },
      ],
      expect.objectContaining({
        title: `Harper merged the pull request for EXP-9`,
      })
    )
  })

  it(`excludes a mapped GitHub actor who is no longer a member without naming them`, async () => {
    h.selectQueue.push(
      // loadIssueMeta
      [issueMeta],
      // deliverableRecipients: the membership gate rejects them
      [],
      // loadPrSessionCandidates: nothing to fall back to either
      [],
      // subscriberRecipients: the outside contributor still subscribes
      [{ userId: `u-gh` }, { userId: `u2` }],
      // deliverableRecipients (deliver) — no actorName lookup
      [{ id: `u2` }]
    )
    h.executeRows.push({ id: `n19`, user_id: `u2` })

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_opened`,
      actorUserId: null,
      githubActorUserId: `u-gh`,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [{ userId: `u2`, data: { notificationId: `n19` } }],
      expect.objectContaining({
        title: `A pull request was opened for EXP-9`,
      })
    )
  })

  it(`respects the exclusion set on the assignee arm too`, async () => {
    noteAgentIssueActivity(issueMeta.id, `u-assignee`)
    h.selectQueue.push(
      // loadIssueMeta: the agent's human is also the assignee
      [{ ...issueMeta, assigneeId: `u-assignee` }],
      // loadPrSessionCandidates
      [],
      // subscriberRecipients: nobody else subscribes
      [{ userId: `u2` }],
      // deliverableRecipients (deliver) — the assignee was never added
      [{ id: `u2` }]
    )
    h.executeRows.push({ id: `n20`, user_id: `u2` })

    fireAndForgetPrNotify({
      issueId: issueMeta.id,
      type: `pr_opened`,
      actorUserId: null,
    })

    await vi.waitFor(() => expect(h.sendToUsers).toHaveBeenCalledTimes(1))
    // The assignee-opt-out lookup never runs either — the set short-circuits
    // the whole arm.
    expect(h.sendToUsers).toHaveBeenCalledWith(
      [{ userId: `u2`, data: { notificationId: `n20` } }],
      expect.anything()
    )
  })
})
