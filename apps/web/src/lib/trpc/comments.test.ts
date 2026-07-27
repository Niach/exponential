import { beforeEach, describe, expect, it, vi } from "vitest"

// REV2-26: comment EDITS must resolve @mentions like the issue-description
// edit path does — delta-based, so only members mentioned in the NEW body get
// subscribed + notified and re-saving an unchanged comment never re-pings.
// The fan-out is mention-only (fireAndForgetIssueMentionNotify): an edit is
// not a new comment, so subscribers must not get an issue_comment ping.

const h = vi.hoisted(() => ({
  isUserAdmin: vi.fn(async () => false),
  resolveTeamAccess: vi.fn(async () => ({ kind: `member` }) as unknown),
  getIssueTeamContext: vi.fn(async () => ({
    issueId: `issue-1`,
    boardId: `proj-1`,
    teamId: `ws-1`,
  })),
  resolveMentions: vi.fn(async (..._args: unknown[]) => [] as string[]),
  ensureSubscribed: vi.fn(async (..._args: unknown[]) => undefined),
  fireAndForgetCommentNotify: vi.fn(),
  fireAndForgetIssueMentionNotify: vi.fn(),
}))

// lib/trpc.ts + lib/admin.ts import db/auth at module scope; runtime here only
// needs the exports to exist.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/admin`, () => ({
  isUserAdmin: h.isUserAdmin,
  assertAdmin: vi.fn(),
}))
vi.mock(`@/lib/team-membership`, () => ({
  resolveTeamAccess: h.resolveTeamAccess,
  getIssueTeamContext: h.getIssueTeamContext,
}))
vi.mock(`@/lib/integrations/mentions`, () => ({
  resolveMentions: h.resolveMentions,
}))
vi.mock(`@/lib/integrations/subscriptions`, () => ({
  ensureSubscribed: h.ensureSubscribed,
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetCommentNotify: h.fireAndForgetCommentNotify,
  fireAndForgetIssueMentionNotify: h.fireAndForgetIssueMentionNotify,
}))

import { commentsRouter } from "@/lib/trpc/comments"
import { extractMentionEmails } from "@/lib/mention-refs"

const COMMENT_ID = `11111111-1111-4111-8111-111111111111`
const ISSUE_ID = `22222222-2222-4222-8222-222222222222`

// The real resolver only returns ids of TEAM MEMBERS; this stand-in keeps the
// same contract over the real `@email` token regex.
const MEMBER_IDS: Record<string, string> = {
  [`alice@example.com`]: `user-alice`,
  [`bob@example.com`]: `user-bob`,
}

const state = {
  // Body the comment row holds BEFORE the edit.
  previousBody: ``,
}

const fakeTx = {
  execute: async () => ({ rows: [{ txid: `42` }] }),
  select: () => ({
    from: () => ({
      where: () => ({
        limit: async () => [{ body: state.previousBody }],
      }),
    }),
  }),
  update: () => ({
    set: (values: Record<string, unknown>) => ({
      where: () => ({
        returning: async () => [{ id: COMMENT_ID, ...values }],
      }),
    }),
  }),
  insert: () => ({
    values: (values: Record<string, unknown>) => ({
      returning: async () => [{ id: COMMENT_ID, ...values }],
    }),
  }),
}

const fakeDb = {
  // loadCommentForMutation's author/team lookup.
  select: () => ({
    from: () => ({
      where: () => ({
        limit: async () => [
          {
            id: COMMENT_ID,
            authorId: `actor`,
            issueId: ISSUE_ID,
            teamId: `ws-1`,
          },
        ],
      }),
    }),
  }),
  transaction: async (fn: (tx: unknown) => Promise<unknown>) => fn(fakeTx),
}

const caller = commentsRouter.createCaller({
  session: { user: { id: `actor` } },
  db: fakeDb,
  request: new Request(`http://localhost/`),
} as never)

describe(`comments.update mention resolution (REV2-26)`, () => {
  beforeEach(() => {
    state.previousBody = ``
    h.resolveMentions.mockReset()
    h.resolveMentions.mockImplementation(async (...args: unknown[]) =>
      extractMentionEmails(args[1] as string)
        .map((email) => MEMBER_IDS[email])
        .filter(Boolean)
    )
    h.ensureSubscribed.mockClear()
    h.fireAndForgetIssueMentionNotify.mockClear()
    h.fireAndForgetCommentNotify.mockClear()
  })

  it(`subscribes and notifies a mention added by the edit`, async () => {
    state.previousBody = `no mentions yet`

    const result = await caller.update({
      id: COMMENT_ID,
      body: `ping @alice@example.com`,
    })

    expect(result.comment.body).toBe(`ping @alice@example.com`)
    expect(h.ensureSubscribed).toHaveBeenCalledTimes(1)
    expect(h.ensureSubscribed.mock.calls[0][1]).toMatchObject({
      issueId: ISSUE_ID,
      userId: `user-alice`,
      teamId: `ws-1`,
      source: `mention`,
    })
    expect(h.fireAndForgetIssueMentionNotify).toHaveBeenCalledWith({
      issueId: ISSUE_ID,
      actorUserId: `actor`,
      mentionedUserIds: [`user-alice`],
    })
    // An edit is not a new comment — subscribers get no issue_comment ping.
    expect(h.fireAndForgetCommentNotify).not.toHaveBeenCalled()
  })

  it(`only fires for NEWLY added mentions`, async () => {
    state.previousBody = `hi @alice@example.com`

    await caller.update({
      id: COMMENT_ID,
      body: `hi @alice@example.com and @bob@example.com`,
    })

    expect(h.ensureSubscribed).toHaveBeenCalledTimes(1)
    expect(h.ensureSubscribed.mock.calls[0][1]).toMatchObject({
      userId: `user-bob`,
    })
    expect(h.fireAndForgetIssueMentionNotify).toHaveBeenCalledWith(
      expect.objectContaining({ mentionedUserIds: [`user-bob`] })
    )
  })

  it(`re-saving the same mentions re-pings nobody`, async () => {
    state.previousBody = `hi @alice@example.com`

    await caller.update({ id: COMMENT_ID, body: `hi @alice@example.com!` })

    expect(h.ensureSubscribed).not.toHaveBeenCalled()
    expect(h.fireAndForgetIssueMentionNotify).not.toHaveBeenCalled()
  })

  it(`removing a mention notifies nobody`, async () => {
    state.previousBody = `hi @alice@example.com`

    await caller.update({ id: COMMENT_ID, body: `hi` })

    expect(h.ensureSubscribed).not.toHaveBeenCalled()
    expect(h.fireAndForgetIssueMentionNotify).not.toHaveBeenCalled()
  })

  it(`create still resolves mentions and notifies via the comment fan-out`, async () => {
    await caller.create({
      issueId: ISSUE_ID,
      body: `welcome @bob@example.com`,
    })

    // Commenter auto-subscribe + the mention subscribe.
    expect(h.ensureSubscribed).toHaveBeenCalledTimes(2)
    expect(h.fireAndForgetCommentNotify).toHaveBeenCalledWith(
      expect.objectContaining({
        issueId: ISSUE_ID,
        actorUserId: `actor`,
        mentionedUserIds: [`user-bob`],
      })
    )
    expect(h.fireAndForgetIssueMentionNotify).not.toHaveBeenCalled()
  })
})
