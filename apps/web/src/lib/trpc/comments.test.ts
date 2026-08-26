import { beforeEach, describe, expect, it, vi } from "vitest"

// REV2-26: comment EDITS must resolve @mentions like the issue-description
// edit path does — delta-based, so only members mentioned in the NEW body get
// subscribed + notified and re-saving an unchanged comment never re-pings.
// The fan-out is mention-only (fireAndForgetIssueMentionNotify): an edit is
// not a new comment, so subscribers must not get an issue_comment ping.

const h = vi.hoisted(() => ({
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
  deleteStorageObjects: vi.fn(async (..._args: unknown[]) => undefined),
}))

// lib/trpc.ts imports db/auth at module scope; runtime here only needs the
// exports to exist.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

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
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  deleteStorageObjects: h.deleteStorageObjects,
}))

import {
  attachments as attachmentsTable,
  comments as commentsTable,
  issues as issuesTable,
} from "@/db/schema"
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
  // Author of the stored comment row (the caller is `actor`).
  authorId: `actor`,
  // FIFO of row sets for tx-scope selects (EXP-554 attachment queries run in
  // the same tx). Empty queue falls back to the previous-body row the update
  // path reads, so the mention tests stay oblivious.
  selectQueue: [] as unknown[][],
  // Team markdown the placeholder rewrite scans before any attachment hard
  // delete (lib/storage/attachment-references), plus what it wrote back.
  issueRows: [] as { id: string; description: string | null }[],
  commentRows: [] as { id: string; body: string }[],
  issueUpdates: [] as Record<string, unknown>[],
  commentUpdates: [] as Record<string, unknown>[],
  // Rows the NON-tx attachments probe sees (the update path's existing-links
  // check, EXP-560).
  dbAttachmentRows: [] as { id: string }[],
}

function resetMarkdownState() {
  state.issueRows = []
  state.commentRows = []
  state.issueUpdates = []
  state.commentUpdates = []
}

const nextSelectRows = () =>
  state.selectQueue.length > 0
    ? state.selectQueue.shift()!
    : [{ body: state.previousBody }]

// The rewrite scan reads `id` + text from issues and comments; every other
// tx-scope select rides the FIFO queue.
function selectRowsFor(fields: Record<string, unknown>, table: unknown) {
  if (table === issuesTable) return state.issueRows
  if (table === commentsTable && `id` in fields) return state.commentRows
  return nextSelectRows()
}

const fakeTx = {
  execute: async () => ({ rows: [{ txid: `42` }] }),
  select: (fields: Record<string, unknown> = {}) => ({
    from: (table?: unknown) => ({
      where: () => {
        // Drizzle builders are awaitable at every stage: `.where()` is awaited
        // directly by the attachment queries and `.limit()` by the body read.
        const rows = selectRowsFor(fields, table)
        return Object.assign(Promise.resolve(rows), {
          limit: async () => rows,
        })
      },
    }),
  }),
  update: (table?: unknown) => ({
    set: (values: Record<string, unknown>) => ({
      where: () => {
        if (table === issuesTable) state.issueUpdates.push(values)
        else if (table === commentsTable) state.commentUpdates.push(values)
        return Object.assign(Promise.resolve(undefined), {
          returning: async () => [{ id: COMMENT_ID, ...values }],
        })
      },
    }),
  }),
  insert: () => ({
    values: (values: Record<string, unknown>) => ({
      returning: async () => [{ id: COMMENT_ID, ...values }],
    }),
  }),
  delete: () => ({ where: async () => undefined }),
}

const fakeDb = {
  // loadCommentForMutation's author/team lookup, plus the update path's
  // existing-attachments probe (EXP-560) keyed by table.
  select: () => ({
    from: (table?: unknown) => ({
      where: () => ({
        limit: async () =>
          table === attachmentsTable
            ? state.dbAttachmentRows
            : [
                {
                  id: COMMENT_ID,
                  authorId: state.authorId,
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
    state.authorId = `actor`
    state.selectQueue = []
    state.dbAttachmentRows = []
    resetMarkdownState()
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

// EXP-398: editing or deleting someone else's words is nobody's business —
// not even a global admin's. The router asks about authorship ONLY; there is
// no admin lookup left to bypass it, and the clients hide the menu to match.
describe(`comments are author-only`, () => {
  beforeEach(() => {
    state.previousBody = `someone else's words`
    state.authorId = `not-actor`
    state.selectQueue = []
    state.dbAttachmentRows = []
    resetMarkdownState()
    h.resolveTeamAccess.mockClear()
  })

  it(`refuses an update by a non-author`, async () => {
    await expect(
      caller.update({ id: COMMENT_ID, body: `rewritten` })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`refuses a delete by a non-author`, async () => {
    await expect(caller.delete({ id: COMMENT_ID })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
  })

  it(`still requires the author to be a current team member`, async () => {
    state.authorId = `actor`
    // Delete's linked-attachments collection pass finds nothing.
    state.selectQueue = [[]]
    await caller.delete({ id: COMMENT_ID })

    expect(h.resolveTeamAccess).toHaveBeenCalledWith(`actor`, `ws-1`, `comment`)
  })
})

// EXP-554: comment attachments link via attachments.comment_id — never inline
// markdown. The router validates the ids, reconciles the linked set on update,
// and hard-deletes linked rows (and their blobs) with the comment.
describe(`comment attachments (EXP-554)`, () => {
  const ATTACHMENT_A = `33333333-3333-4333-8333-333333333333`
  const ATTACHMENT_B = `44444444-4444-4444-8444-444444444444`
  const OTHER_COMMENT = `55555555-5555-4555-8555-555555555555`

  const uploadedRow = (id: string, overrides?: Record<string, unknown>) => ({
    id,
    issueId: ISSUE_ID,
    commentId: null,
    uploaderId: `actor`,
    ...overrides,
  })

  beforeEach(() => {
    state.previousBody = ``
    state.authorId = `actor`
    state.selectQueue = []
    state.dbAttachmentRows = []
    resetMarkdownState()
    h.getIssueTeamContext.mockImplementation(async () => ({
      issueId: ISSUE_ID,
      boardId: `proj-1`,
      teamId: `ws-1`,
    }))
    h.resolveMentions.mockReset()
    h.resolveMentions.mockImplementation(async () => [])
    h.fireAndForgetCommentNotify.mockClear()
    h.deleteStorageObjects.mockClear()
  })

  it(`create links uploaded attachments and reports the count to notify`, async () => {
    state.selectQueue = [
      // syncCommentAttachmentsInTx: the requested rows, then currently linked.
      [uploadedRow(ATTACHMENT_A)],
      [],
    ]

    await caller.create({
      issueId: ISSUE_ID,
      body: ``,
      attachmentIds: [ATTACHMENT_A],
    })

    expect(h.fireAndForgetCommentNotify).toHaveBeenCalledWith(
      expect.objectContaining({ attachmentCount: 1 })
    )
  })

  it(`rejects an empty comment with no attachments`, async () => {
    await expect(
      caller.create({ issueId: ISSUE_ID, body: `   `, attachmentIds: [] })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`rejects attachments of another issue`, async () => {
    state.selectQueue = [
      [uploadedRow(ATTACHMENT_A, { issueId: `66666666-6666-4666-8666-666666666666` })],
    ]

    await expect(
      caller.create({
        issueId: ISSUE_ID,
        body: `hi`,
        attachmentIds: [ATTACHMENT_A],
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`rejects attachments uploaded by someone else`, async () => {
    state.selectQueue = [
      [uploadedRow(ATTACHMENT_A, { uploaderId: `someone-else` })],
    ]

    await expect(
      caller.create({
        issueId: ISSUE_ID,
        body: `hi`,
        attachmentIds: [ATTACHMENT_A],
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`rejects attachments already claimed by another comment`, async () => {
    state.selectQueue = [
      [uploadedRow(ATTACHMENT_A, { commentId: OTHER_COMMENT })],
    ]

    await expect(
      caller.create({
        issueId: ISSUE_ID,
        body: `hi`,
        attachmentIds: [ATTACHMENT_A],
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`update accepts an empty body when attachmentIds is omitted but links exist (EXP-560)`, async () => {
    state.previousBody = `old`
    state.dbAttachmentRows = [{ id: ATTACHMENT_A }]

    const result = await caller.update({ id: COMMENT_ID, body: `` })

    expect(result.comment.body).toBe(``)
  })

  it(`update refuses an empty body when attachmentIds is omitted and nothing is linked`, async () => {
    state.previousBody = `old`
    state.dbAttachmentRows = []

    await expect(
      caller.update({ id: COMMENT_ID, body: `` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`update refuses an empty body with an explicitly empty attachment set`, async () => {
    state.previousBody = `old`
    state.dbAttachmentRows = [{ id: ATTACHMENT_A }]

    await expect(
      caller.update({ id: COMMENT_ID, body: ``, attachmentIds: [] })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`update hard-deletes linked rows dropped from the list and reclaims blobs`, async () => {
    state.previousBody = `old`
    state.selectQueue = [
      // Previous-body read.
      [{ body: `old` }],
      // sync: requested rows (the kept attachment, already linked)...
      [uploadedRow(ATTACHMENT_A, { commentId: COMMENT_ID })],
      // ...then currently linked: kept + one to remove.
      [
        { id: ATTACHMENT_A, filename: `a.png`, storageKey: `key-a` },
        { id: ATTACHMENT_B, filename: `b.png`, storageKey: `key-b` },
      ],
    ]

    await caller.update({
      id: COMMENT_ID,
      body: `new`,
      attachmentIds: [ATTACHMENT_A],
    })

    expect(h.deleteStorageObjects).toHaveBeenCalledWith([`key-b`])
  })

  it(`delete cascades linked attachments and reclaims their blobs`, async () => {
    state.selectQueue = [
      [
        { id: ATTACHMENT_A, filename: `a.png`, storageKey: `key-a` },
        { id: ATTACHMENT_B, filename: `b.png`, storageKey: `key-b` },
      ],
    ]

    await caller.delete({ id: COMMENT_ID })

    expect(h.deleteStorageObjects).toHaveBeenCalledWith([`key-a`, `key-b`])
  })

  // The link gate only asks for same issue + same uploader + unclaimed, so an
  // attachment that is ALSO embedded inline (issue description, or a comment
  // body written by an old client) can end up linked to a comment. Hard
  // deleting it without the attachments.delete placeholder rewrite would leave
  // a dead `![](/api/attachments/{id})` behind, and the next issues.update
  // would 400 on the round-trip guard.
  it(`unlinking via update rewrites inline references to the placeholder`, async () => {
    state.previousBody = `old`
    state.selectQueue = [
      [{ body: `old` }],
      [uploadedRow(ATTACHMENT_A, { commentId: COMMENT_ID })],
      [
        { id: ATTACHMENT_A, filename: `a.png`, storageKey: `key-a` },
        { id: ATTACHMENT_B, filename: `dropped.png`, storageKey: `key-b` },
      ],
    ]
    state.issueRows = [
      {
        id: ISSUE_ID,
        description: `before ![shot](/api/attachments/${ATTACHMENT_B}) after`,
      },
    ]
    state.commentRows = [
      { id: OTHER_COMMENT, body: `old client ![](/api/attachments/${ATTACHMENT_B})` },
    ]

    await caller.update({
      id: COMMENT_ID,
      body: `new`,
      attachmentIds: [ATTACHMENT_A],
    })

    expect(state.issueUpdates).toEqual([
      { description: `before *(deleted image: shot)* after` },
    ])
    // No alt text — the placeholder falls back to the stored filename.
    expect(state.commentUpdates).toContainEqual({
      body: `old client *(deleted image: dropped.png)*`,
    })
    expect(h.deleteStorageObjects).toHaveBeenCalledWith([`key-b`])
  })

  it(`delete cascade rewrites inline references to the placeholder`, async () => {
    state.selectQueue = [
      [{ id: ATTACHMENT_A, filename: `cascade.png`, storageKey: `key-a` }],
    ]
    state.issueRows = [
      {
        id: ISSUE_ID,
        description: `see ![](/api/attachments/${ATTACHMENT_A}?w=480)`,
      },
    ]

    await caller.delete({ id: COMMENT_ID })

    expect(state.issueUpdates).toEqual([
      { description: `see *(deleted image: cascade.png)*` },
    ])
    expect(h.deleteStorageObjects).toHaveBeenCalledWith([`key-a`])
  })

  it(`leaves a body the exact markdown parser does not match alone`, async () => {
    state.selectQueue = [
      [{ id: ATTACHMENT_A, filename: `cascade.png`, storageKey: `key-a` }],
    ]
    // Matched by the LIKE prefilter (bare id in the text) but not by the
    // markdown parser — nothing to rewrite.
    state.issueRows = [{ id: ISSUE_ID, description: `mentions ${ATTACHMENT_A}` }]

    await caller.delete({ id: COMMENT_ID })

    expect(state.issueUpdates).toEqual([])
  })
})
