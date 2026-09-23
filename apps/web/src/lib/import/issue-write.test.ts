import { describe, expect, it } from "vitest"
import {
  attributionLine,
  planIssueWrite,
  rewriteAssetRefs,
  type IssueWriteContext,
  type IssueWriteIds,
} from "@/lib/import/issue-write"
import { bundleFixture } from "@/lib/import/fixtures"

// EXP-630: the pure row planner. Timestamps pass through, completedAt
// follows our rule, the status dual-write uses the target row's anchor, an
// unmapped comment author becomes the importer + an attribution line,
// threads re-root, fetched assets rewrite to the canonical URL, history
// becomes events only when opted in.

const IMPORTER = `importer-id`

function context(overrides: Partial<IssueWriteContext> = {}): IssueWriteContext {
  return {
    teamId: `team-1`,
    importerId: IMPORTER,
    sourceLabel: `Test Tracker`,
    importHistory: true,
    boards: new Map([[`b-main`, { boardId: `board-1`, numbering: `preserve` as const }]]),
    statuses: new Map([
      [`st-open`, { id: `s-backlog`, name: `Backlog`, category: `backlog`, builtinKey: `backlog` }],
      [`st-doing`, { id: `s-doing`, name: `Doing`, category: `started`, builtinKey: null }],
      [`st-done`, { id: `s-done`, name: `Done`, category: `completed`, builtinKey: `done` }],
      [`st-dup`, { id: `s-duplicate`, name: `Duplicate`, category: `duplicate`, builtinKey: `duplicate` }],
    ]),
    labels: new Map([
      [`lb-bug`, `label-bug`],
      [`lb-new`, null],
    ]),
    users: new Map([
      [`u-h`, { userId: `member-hannes`, name: `Hannes`, email: `hannes.robier@youspi.com` }],
      [`u-x`, { userId: null, name: `Stranger`, email: `stranger@example.com` }],
    ]),
    cancelledStatus: { id: `s-cancelled`, name: `Cancelled`, category: `cancelled`, builtinKey: `cancelled` },
    ...overrides,
  }
}

function ids(issueKey = `i-1`): IssueWriteIds {
  const issue = bundleFixture().issues.find((row) => row.key === issueKey)!
  return {
    issueId: `issue-${issueKey}`,
    commentIds: new Map(issue.comments.map((comment) => [comment.key, `id-${comment.key}`])),
    attachmentIds: new Map(issue.assets.map((asset) => [asset.key, `att-${asset.key}`])),
  }
}

describe(`planIssueWrite`, () => {
  const issue = bundleFixture().issues[0]!

  it(`keeps the source timestamps and the preserved number`, () => {
    const planned = planIssueWrite(issue, context(), ids(), { availableAssetKeys: new Set([`a-1`]), canonicalAvailable: false })
    expect(planned.issue).toMatchObject({
      id: `issue-i-1`,
      boardId: `board-1`,
      number: 10,
      title: `Ten`,
      priority: `high`,
      assigneeId: `member-hannes`,
      creatorId: null,
      status: `done`,
      statusId: `s-done`,
    })
    expect(planned.issue.createdAt.toISOString()).toBe(`2025-01-01T00:00:00.000Z`)
    expect(planned.issue.updatedAt.toISOString()).toBe(`2025-02-01T00:00:00.000Z`)
    expect(planned.issue.completedAt?.toISOString()).toBe(`2025-02-01T00:00:00.000Z`)
  })

  it(`drops the number under allocate and clears completedAt for a non-terminal target`, () => {
    const ctx = context({
      boards: new Map([[`b-main`, { boardId: `board-1`, numbering: `allocate` }]]),
      statuses: new Map([
        ...context().statuses,
        [`st-done`, { id: `s-doing`, name: `Doing`, category: `started`, builtinKey: null }],
      ]),
    })
    const planned = planIssueWrite(issue, ctx, ids(), { availableAssetKeys: new Set(), canonicalAvailable: false })
    expect(planned.issue.number).toBeNull()
    expect(planned.issue.completedAt).toBeNull()
    // A custom started row anchors to in_progress.
    expect(planned.issue).toMatchObject({ status: `in_progress`, statusId: `s-doing` })
  })

  it(`stamps completedAt from updatedAt when the source has none but the target is terminal`, () => {
    const planned = planIssueWrite(
      { ...issue, completedAt: null },
      context(),
      ids(),
      { availableAssetKeys: new Set(), canonicalAvailable: false }
    )
    expect(planned.issue.completedAt?.toISOString()).toBe(`2025-02-01T00:00:00.000Z`)
  })

  it(`rewrites fetched asset refs to the canonical attachment URL and keeps the rest`, () => {
    const planned = planIssueWrite(issue, context(), ids(), { availableAssetKeys: new Set([`a-1`]), canonicalAvailable: false })
    expect(planned.issue.description).toBe(`See ![a](/api/attachments/att-a-1)`)
    expect(planned.attachments).toEqual([
      expect.objectContaining({ id: `att-a-1`, filename: `a.png`, commentId: null, url: `/api/attachments/att-a-1` }),
    ])
    expect(planned.warnings).toEqual([])

    const missing = planIssueWrite(issue, context(), ids(), { availableAssetKeys: new Set(), canonicalAvailable: false })
    expect(missing.issue.description).toBe(`See ![a](https://files.example.com/a.png)`)
    expect(missing.attachments).toEqual([])
    expect(missing.warnings[0]).toMatch(/could not fetch a\.png/)
  })

  it(`skips unmapped labels`, () => {
    const planned = planIssueWrite(issue, context(), ids(), { availableAssetKeys: new Set(), canonicalAvailable: false })
    expect(planned.labels).toEqual([
      { issueId: `issue-i-1`, labelId: `label-bug`, teamId: `team-1`, boardId: `board-1` },
    ])
  })

  it(`attributes an unmapped author to the importer with ONE attribution line, and keeps the mapped one`, () => {
    const planned = planIssueWrite(issue, context(), ids(), { availableAssetKeys: new Set(), canonicalAvailable: false })
    const [root, reply] = planned.comments
    expect(root).toMatchObject({ id: `id-c-1`, authorId: IMPORTER, parentId: null })
    expect(root!.body).toBe(
      `*Imported from Test Tracker — originally by Stranger (stranger@example.com), 2 Jan 2025*\n\nFrom a stranger`
    )
    expect(root!.createdAt.toISOString()).toBe(`2025-01-02T00:00:00.000Z`)
    expect(reply).toMatchObject({ id: `id-c-2`, authorId: `member-hannes`, parentId: `id-c-1`, body: `Reply` })
  })

  it(`re-roots a reply to a reply under the thread root and orders roots first`, () => {
    const threaded = {
      ...issue,
      comments: [
        { key: `c-3`, authorKey: `u-h`, body: `Deep`, createdAt: `2025-01-01T00:00:00.000Z`, parentKey: `c-2` },
        ...issue.comments,
      ],
    }
    const planned = planIssueWrite(
      threaded,
      context(),
      { ...ids(), commentIds: new Map([...ids().commentIds, [`c-3`, `id-c-3`]]) },
      { availableAssetKeys: new Set(), canonicalAvailable: false }
    )
    expect(planned.comments.map((comment) => [comment.id, comment.parentId])).toEqual([
      [`id-c-1`, null],
      [`id-c-3`, `id-c-1`],
      [`id-c-2`, `id-c-1`],
    ])
  })

  it(`writes a created event at the source date and history events only when opted in`, () => {
    const withHistory = planIssueWrite(issue, context(), ids(), { availableAssetKeys: new Set(), canonicalAvailable: false })
    expect(withHistory.events.map((event) => event.type)).toEqual([`created`, `status_changed`])
    expect(withHistory.events[0]!.createdAt.toISOString()).toBe(`2025-01-01T00:00:00.000Z`)
    expect(withHistory.events[0]!.payload).toEqual({
      status: `done`,
      statusId: `s-done`,
      priority: `high`,
      source: `user`,
    })
    expect(withHistory.events[1]).toMatchObject({
      actorUserId: `member-hannes`,
      payload: { fromStatusId: `s-backlog`, toStatusId: `s-doing`, fromName: `Backlog`, toName: `Doing` },
    })
    const without = planIssueWrite(issue, context({ importHistory: false }), ids(), {
      availableAssetKeys: new Set(),
      canonicalAvailable: false,
    })
    expect(without.events.map((event) => event.type)).toEqual([`created`])
  })

  it(`subscribes the mapped assignee and creator only, and maps every row`, () => {
    const planned = planIssueWrite(issue, context(), ids(), { availableAssetKeys: new Set([`a-1`]), canonicalAvailable: false })
    expect(planned.subscribers).toEqual([{ userId: `member-hannes`, source: `assignee` }])
    expect(planned.map).toEqual([
      { kind: `issue`, externalId: `i-1`, externalRef: `MAIN-10`, localId: `issue-i-1` },
      { kind: `comment`, externalId: `c-1`, externalRef: null, localId: `id-c-1` },
      { kind: `comment`, externalId: `c-2`, externalRef: null, localId: `id-c-2` },
      { kind: `attachment`, externalId: `a-1`, externalRef: `https://files.example.com/a.png`, localId: `att-a-1` },
    ])
  })

  it(`lands a duplicate with no canonical on the cancelled builtin, and keeps duplicate when it has one`, () => {
    const dup = bundleFixture().issues[1]!
    const orphan = planIssueWrite(dup, context(), ids(`i-2`), {
      availableAssetKeys: new Set(),
      canonicalAvailable: false,
    })
    expect(orphan.issue).toMatchObject({ status: `cancelled`, statusId: `s-cancelled` })
    expect(orphan.issue.completedAt?.toISOString()).toBe(`2025-01-06T00:00:00.000Z`)
    expect(orphan.warnings[0]).toMatch(/MAIN-11: marked duplicate/)
    const linked = planIssueWrite(dup, context(), ids(`i-2`), {
      availableAssetKeys: new Set(),
      canonicalAvailable: true,
    })
    expect(linked.issue).toMatchObject({ status: `duplicate`, statusId: `s-duplicate` })
    expect(linked.warnings).toEqual([])
  })

  it(`throws when the board or status is unresolved`, () => {
    expect(() =>
      planIssueWrite(issue, context({ boards: new Map() }), ids(), { availableAssetKeys: new Set(), canonicalAvailable: false })
    ).toThrow(/No target board/)
    expect(() =>
      planIssueWrite(issue, context({ statuses: new Map() }), ids(), { availableAssetKeys: new Set(), canonicalAvailable: false })
    ).toThrow(/No target status/)
  })
})

describe(`helpers`, () => {
  it(`rewriteAssetRefs replaces every occurrence of every ref`, () => {
    const text = `![a](u1) and [b](u1) and u2`
    expect(rewriteAssetRefs(text, new Map([[`u1`, `/x`], [`u2`, `/y`]]))).toBe(`![a](/x) and [b](/x) and /y`)
  })

  it(`attributionLine names the author, the email and the date`, () => {
    expect(attributionLine(`Linear`, { name: `Hannes Robier`, email: `h@x.com` }, `2026-06-09T12:00:00.000Z`)).toBe(
      `*Imported from Linear — originally by Hannes Robier (h@x.com), 9 Jun 2026*`
    )
    expect(attributionLine(`Linear`, null, `2026-06-09T12:00:00.000Z`)).toBe(
      `*Imported from Linear — originally by an unknown user, 9 Jun 2026*`
    )
    expect(attributionLine(`Linear`, { name: `h@x.com`, email: `h@x.com` }, `2026-06-09T12:00:00.000Z`)).toBe(
      `*Imported from Linear — originally by h@x.com, 9 Jun 2026*`
    )
  })
})
