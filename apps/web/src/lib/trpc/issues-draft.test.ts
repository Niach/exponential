import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-878: `issues.create({ draftId })` — the hand-over from a draft to the
// issue it becomes. What this locks:
//   * the "no images before the issue exists" refusal RELAXES for a draft:
//     its uploads were eager, so the description already carries final
//     `/api/attachments/{id}` URLs — but every one of them must be a row the
//     DRAFT owns;
//   * on create, EVERY draft-owned attachment is reparented (issue_id +
//     board_id set, draft_id cleared) and the draft row is deleted, in the
//     SAME transaction as the insert — so one txId covers the whole move;
//   * the draft lookup is user-scoped and a MISSING row is tolerated: the
//     client mints the id when the dialog opens, so a plain create that never
//     uploaded anything legitimately names a draft that was never written;
//   * the draft's team must be the target board's team (the reparent moves
//     attachments, and their quota, so it must never cross a team), and the
//     reparent writes team_id explicitly;
//   * image ownership is only ever proven through the caller's OWN draft row:
//     an unowned draft id never admits an attachment, even one that really
//     hangs off that draft.

const h = vi.hoisted(() => ({
  draftRows: [] as { id: string; teamId: string }[],
  ownedAttachmentIds: [] as string[],
  hasMarkdownImages: vi.fn((_text: string) => false),
  canonicalize: vi.fn((text: string) => text),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/team-membership`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/team-membership")>()
  return {
    ...actual,
    getBoardTeamId: vi.fn(async () => ({ id: `proj-1`, teamId: `ws-1` })),
    resolveTeamAccess: vi.fn(async () => ({
      kind: `member`,
      team: { id: `ws-1` },
      member: { role: `member`, userId: `actor`, teamId: `ws-1` },
    })),
    assertAssigneeInTeam: vi.fn(async () => undefined),
    getSoleHumanMemberId: vi.fn(async () => null),
  }
})

vi.mock(`@/lib/integrations/github-pr`, () => ({
  fetchPullFiles: vi.fn(),
  mergePullRequest: vi.fn(),
  resolveRepoToken: vi.fn(),
  GitHubMergeError: class extends Error {},
}))
vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: () => false,
  resolveRepoInstallationToken: vi.fn(),
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrMergeState: vi.fn(),
}))
vi.mock(`@/lib/trpc/repositories`, () => ({
  resolveBoardRepository: vi.fn(),
}))
vi.mock(`@/lib/storage/issue-attachments`, () => ({
  canonicalizeMarkdownImageUrls: (text: string) => h.canonicalize(text),
  // The real extractor: the ownership guard is only meaningful if the URLs
  // are really parsed out of the markdown.
  extractAttachmentIdsFromDescription: (text: string) => {
    const ids: string[] = []
    const invalid: string[] = []
    for (const match of text.matchAll(/!\[[^\]]*]\(([^)\s]+)\)/g)) {
      const url = match[1]
      const attachment = /^\/api\/attachments\/([0-9a-f-]{36})/i.exec(url)
      if (attachment) ids.push(attachment[1])
      else invalid.push(url)
    }
    return { attachmentIds: ids, invalidUrls: invalid }
  },
  hasMarkdownImages: (text: string) => h.hasMarkdownImages(text),
}))
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  collectIssueAttachmentStorageKeysInTx: vi.fn(),
  deleteStorageObjects: vi.fn(),
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetAssignmentNotify: vi.fn(),
  fireAndForgetStatusChangeNotify: vi.fn(),
  fireAndForgetReporterResolution: vi.fn(),
  fireAndForgetIssueMentionNotify: vi.fn(),
}))
vi.mock(`@/lib/integrations/subscriptions`, () => ({
  ensureSubscribed: vi.fn(),
}))
vi.mock(`@/lib/integrations/activity`, () => ({
  recordIssueEvent: vi.fn(),
}))

import { attachments, issueDrafts } from "@/db/schema"
import { issuesRouter } from "@/lib/trpc/issues"

const BOARD_ID = `11111111-1111-4111-8111-111111111111`
const ISSUE_ID = `22222222-2222-4222-8222-222222222222`
const DRAFT_ID = `33333333-3333-4333-8333-333333333333`
const ATT_ID = `44444444-4444-4444-8444-444444444444`

const insertedIssues: Array<Record<string, unknown>> = []
const attachmentUpdates: Array<Record<string, unknown>> = []
const deletedTables: unknown[] = []

function thenable(rows: unknown[]) {
  return {
    then: (resolve: (value: unknown[]) => unknown, reject?: unknown) =>
      Promise.resolve(rows).then(resolve, reject as never),
    limit: async (count: number) => rows.slice(0, count),
  }
}

// The pre-transaction reads (draft lookup + attachment ownership probe) run on
// `ctx.db`, the writes inside the transaction handle.
const readDb = {
  select: (fields: Record<string, unknown>) => ({
    from: (table: unknown) => ({
      where: () => {
        if (table === issueDrafts) return thenable(h.draftRows)
        if (table === attachments) {
          return thenable(h.ownedAttachmentIds.map((id) => ({ id })))
        }
        void fields
        return thenable([])
      },
    }),
  }),
}

const tx = {
  execute: vi.fn(async () => ({ rows: [{ txid: `42` }] })),
  insert: vi.fn(() => ({
    values: (values: Record<string, unknown>) => {
      insertedIssues.push(values)
      const returning = async () => [
        {
          id: ISSUE_ID,
          identifier: `EXP-1`,
          boardId: values.boardId,
          ...values,
        },
      ]
      return {
        returning,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        then: (res: any, rej: any) => Promise.resolve().then(res, rej),
      }
    },
  })),
  update: vi.fn((table: unknown) => ({
    set: (values: Record<string, unknown>) => ({
      where: async () => {
        if (table === attachments) attachmentUpdates.push(values)
      },
    }),
  })),
  delete: vi.fn((table: unknown) => ({
    where: async () => {
      deletedTables.push(table)
    },
  })),
}

const transaction = vi.fn(async (fn: (t: typeof tx) => Promise<unknown>) =>
  fn(tx)
)

const caller = issuesRouter.createCaller({
  session: { user: { id: `actor` } },
  db: { ...readDb, transaction },
  request: new Request(`http://localhost:5173/api/trpc/issues.create`),
} as never)

describe(`issues.create with a draft (EXP-878)`, () => {
  beforeEach(() => {
    insertedIssues.length = 0
    attachmentUpdates.length = 0
    deletedTables.length = 0
    h.draftRows = [{ id: DRAFT_ID, teamId: `ws-1` }]
    h.ownedAttachmentIds = []
    h.hasMarkdownImages.mockReset()
    h.hasMarkdownImages.mockReturnValue(false)
    h.canonicalize.mockReset()
    h.canonicalize.mockImplementation((text: string) => text)
    transaction.mockClear()
  })

  it(`reparents the draft's attachments and deletes the draft row`, async () => {
    h.ownedAttachmentIds = [ATT_ID]

    const result = await caller.create({
      boardId: BOARD_ID,
      title: `From a draft`,
      description: `![shot](/api/attachments/${ATT_ID})`,
      draftId: DRAFT_ID,
    })

    expect(insertedIssues[0]?.description).toBe(
      `![shot](/api/attachments/${ATT_ID})`
    )
    // ALL draft-owned rows move — files as well as inline images — and
    // writing board_id lets the populate trigger derive the board mirrors.
    // team_id is written explicitly as well, never left at the draft's.
    expect(attachmentUpdates).toEqual([
      { issueId: ISSUE_ID, boardId: BOARD_ID, teamId: `ws-1`, draftId: null },
    ])
    expect(deletedTables).toEqual([issueDrafts])
    // One transaction, so one txId covers insert + reparent + draft delete.
    expect((result as { txId: number }).txId).toBe(42)
  })

  // The plain-create guard must not be weakened by the draft path existing:
  // an image the draft does not own would survive the reparent as a
  // permanently broken embed.
  it(`refuses an image the draft does not own`, async () => {
    h.ownedAttachmentIds = []

    await expect(
      caller.create({
        boardId: BOARD_ID,
        title: `Stolen`,
        description: `![shot](/api/attachments/${ATT_ID})`,
        draftId: DRAFT_ID,
      })
    ).rejects.toThrow(/Images can only be added after the issue is created/i)
    expect(insertedIssues).toEqual([])
  })

  // The draft is the caller's, but it was opened on a board of ANOTHER team:
  // adopting it here would move its attachments across the team boundary.
  it(`refuses a draft that belongs to a different team than the board`, async () => {
    h.draftRows = [{ id: DRAFT_ID, teamId: `ws-2` }]
    h.ownedAttachmentIds = [ATT_ID]

    await expect(
      caller.create({
        boardId: BOARD_ID,
        title: `Cross-team`,
        description: `![shot](/api/attachments/${ATT_ID})`,
        draftId: DRAFT_ID,
      })
    ).rejects.toThrow(/draft belongs to a different team/i)
    expect(insertedIssues).toEqual([])
    expect(attachmentUpdates).toEqual([])
    expect(deletedTables).toEqual([])
  })

  // The ownership probe is keyed on draft_id alone, so it must not run for a
  // draft the caller does not own: here the attachment row DOES hang off the
  // named draft (somebody else's), and the create still refuses it.
  it(`refuses an image hanging off a draft the caller does not own`, async () => {
    h.draftRows = []
    h.ownedAttachmentIds = [ATT_ID]

    await expect(
      caller.create({
        boardId: BOARD_ID,
        title: `Guessed draft id`,
        description: `![shot](/api/attachments/${ATT_ID})`,
        draftId: DRAFT_ID,
      })
    ).rejects.toThrow(/Images can only be added after the issue is created/i)
    expect(insertedIssues).toEqual([])
  })

  it(`refuses a non-attachment image URL even on the draft path`, async () => {
    await expect(
      caller.create({
        boardId: BOARD_ID,
        title: `Blob`,
        description: `![shot](blob:abc-123)`,
        draftId: DRAFT_ID,
      })
    ).rejects.toThrow(/Images can only be added after the issue is created/i)
  })

  // The dialog mints the draft id when it OPENS; a create that never
  // triggered an eager upload names an id with no row behind it. That is
  // "nothing to reparent, nothing to delete", not an error.
  it(`tolerates a draftId whose row was never written`, async () => {
    h.draftRows = []

    const result = await caller.create({
      boardId: BOARD_ID,
      title: `Never uploaded`,
      draftId: DRAFT_ID,
    })

    expect((result as { issue: { id: string } }).issue.id).toBe(ISSUE_ID)
    // Nothing is reparented and nothing is deleted — crucially, the
    // user-scoped lookup is also what stops a GUESSED id from having somebody
    // else's attachments stolen by the reparent.
    expect(attachmentUpdates).toEqual([])
    expect(deletedTables).toEqual([])
  })

  it(`still applies the image guard when the draft row is missing`, async () => {
    h.draftRows = []
    h.ownedAttachmentIds = []

    await expect(
      caller.create({
        boardId: BOARD_ID,
        title: `No row, but an image`,
        description: `![shot](/api/attachments/${ATT_ID})`,
        draftId: DRAFT_ID,
      })
    ).rejects.toThrow(/Images can only be added after the issue is created/i)
  })

  // Without a draftId nothing changes: images are still refused outright.
  it(`keeps the plain-create refusal when no draft is named`, async () => {
    h.hasMarkdownImages.mockReturnValue(true)

    await expect(
      caller.create({
        boardId: BOARD_ID,
        title: `Plain`,
        description: `![shot](/api/attachments/${ATT_ID})`,
      })
    ).rejects.toThrow(/Images can only be added after the issue is created/i)
  })
})
