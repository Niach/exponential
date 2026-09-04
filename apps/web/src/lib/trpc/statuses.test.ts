import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-314 statuses router: member-gated CRUD over per-team custom statuses.
// Locked builtins reject update/delete; duplicate is a fixed category (no
// creates, no reassignment target); the started cap serializes on the
// category lock; delete with referencing issues demands a replacement and
// reassigns with the same completedAt derivations as any status write —
// recording events but firing NO notifications. Fake-db harness mirrors
// issues-bulk.test.ts (FIFO select queue + recording write chains).

const h = vi.hoisted(() => ({
  resolveTeamAccess: vi.fn(async (..._args: unknown[]) => ({}) as unknown),
  recordIssueEvent: vi.fn(),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/team-membership`, () => ({
  resolveTeamAccess: h.resolveTeamAccess,
  assertAssigneeInTeam: vi.fn(),
  assertIssueAccess: vi.fn(),
  assertTeamMember: vi.fn(),
  getIssueTeamContext: vi.fn(),
  getBoardTeamId: vi.fn(),
  getSoleHumanMemberId: vi.fn(),
  getUserTeamIds: vi.fn(),
}))

// Stub the integration modules side-effect-free (historical: statuses.ts
// used to import applyStatusDerivations from issues.ts, which pulls
// issues.ts's module-scope integrations; it now lives in the dependency-free
// lib/status-derivations.ts).
vi.mock(`@/lib/integrations/github-pr`, () => ({
  closePullRequest: vi.fn(),
  fetchPullFiles: vi.fn(),
  mergePullRequest: vi.fn(),
  GitHubMergeError: class extends Error {},
}))
vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: () => false,
  resolveRepoInstallationTokenInfo: vi.fn(),
}))
vi.mock(`@/lib/trpc/integrations`, () => ({
  isInstallationLinkedToTeam: vi.fn(),
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrClosedState: vi.fn(),
  applyPrMergeState: vi.fn(),
}))
vi.mock(`@/lib/storage/issue-attachments`, () => ({
  canonicalizeMarkdownImageUrls: vi.fn(),
  extractAttachmentIdsFromDescription: vi.fn(),
  hasMarkdownImages: () => false,
}))
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  collectIssueAttachmentStorageKeysInTx: vi.fn(async () => []),
  deleteStorageObjects: vi.fn(),
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetAssignmentNotify: vi.fn(),
  fireAndForgetIssueMentionNotify: vi.fn(),
  fireAndForgetStatusChangeNotify: vi.fn(),
  fireAndForgetReporterResolution: vi.fn(),
}))
vi.mock(`@/lib/integrations/mentions`, () => ({ resolveMentions: vi.fn() }))
vi.mock(`@/lib/integrations/subscriptions`, () => ({
  ensureSubscribed: vi.fn(),
}))
// EXP-736: statuses.delete's reassignment clears duplicate links through
// applyStatusDerivations, so the mirror sync rides this transaction. Its SQL
// has its own tests (lib/issue-relations.test.ts); this file only cares that
// the reassignment happened.
vi.mock(`@/lib/issue-relations`, () => ({
  syncDuplicateMirror: vi.fn(async () => undefined),
}))
vi.mock(`@/lib/integrations/activity`, () => ({
  recordIssueEvent: h.recordIssueEvent,
}))

import { statusesRouter } from "@/lib/trpc/statuses"

const TEAM = `11111111-1111-4111-8111-111111111111`

function uuid(n: number): string {
  return `00000000-0000-4000-8000-${String(n).padStart(12, `0`)}`
}
const STATUS_A = uuid(1)
const STATUS_B = uuid(2)
const ISSUE_1 = uuid(11)
const ISSUE_2 = uuid(12)

const selectQueue: unknown[][] = []

function selectChain(): Promise<unknown[]> & Record<string, () => unknown> {
  const p = Promise.resolve(selectQueue.shift() ?? []) as Promise<unknown[]> &
    Record<string, () => unknown>
  for (const m of [`from`, `where`, `innerJoin`, `limit`, `orderBy`, `for`]) {
    p[m] = () => p
  }
  return p
}

const updates: { set: Record<string, unknown> }[] = []
const deletes: number[] = []
const inserts: { values: Record<string, unknown> }[] = []

const fakeDb = {
  select: vi.fn(() => selectChain()),
  insert: (_table: unknown) => ({
    values: (values: Record<string, unknown>) => ({
      returning: async () => {
        inserts.push({ values })
        return [{ id: STATUS_A, ...values }]
      },
    }),
  }),
  update: (_table: unknown) => ({
    set: (values: Record<string, unknown>) => ({
      where: (_where: unknown) => {
        updates.push({ set: values })
        // EXP-707: statuses.update echoes the row via .returning(); the
        // bare await (thenable) keeps the other writers working.
        const result = Promise.resolve() as Promise<void> & {
          returning: () => Promise<Record<string, unknown>[]>
        }
        result.returning = async () => [{ id: `updated-status`, ...values }]
        return result
      },
    }),
  }),
  delete: (_table: unknown) => ({
    where: (_where: unknown) => {
      deletes.push(1)
      return Promise.resolve()
    },
  }),
  execute: vi.fn(async () => ({ rows: [{ txid: `77` }] })),
  transaction: vi.fn(
    async (fn: (tx: typeof fakeDb) => Promise<unknown>): Promise<unknown> =>
      fn(fakeDb)
  ),
}

const caller = statusesRouter.createCaller({
  session: { user: { id: `actor` } },
  db: fakeDb,
  request: new Request(`http://localhost/`),
} as never)

function statusRow(overrides: Record<string, unknown> = {}) {
  return {
    id: STATUS_A,
    teamId: TEAM,
    category: `started`,
    name: `Rückfrage`,
    color: `#EF4444`,
    sortOrder: 3,
    builtinKey: null,
    createdAt: new Date(),
    updatedAt: new Date(),
    ...overrides,
  }
}

beforeEach(() => {
  selectQueue.length = 0
  updates.length = 0
  deletes.length = 0
  inserts.length = 0
  h.resolveTeamAccess.mockClear()
  h.recordIssueEvent.mockClear()
})

describe(`statuses.referencingCount`, () => {
  it(`returns the server-authoritative count as a read-gated query`, async () => {
    selectQueue.push([{ id: STATUS_A }], [{ count: 3 }])
    await expect(
      caller.referencingCount({ teamId: TEAM, statusId: STATUS_A })
    ).resolves.toEqual({ count: 3 })
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(`actor`, TEAM, `read`)
  })

  it(`404s a status outside the team`, async () => {
    selectQueue.push([])
    await expect(
      caller.referencingCount({ teamId: TEAM, statusId: STATUS_A })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
  })
})

describe(`statuses.create`, () => {
  it(`appends a custom status after the category's max sort order`, async () => {
    selectQueue.push(
      // lockCategory: two started builtins
      [
        { id: uuid(21), sortOrder: 1 },
        { id: uuid(22), sortOrder: 2 },
      ],
      // CI name pre-check: no clash
      []
    )
    const result = await caller.create({
      teamId: TEAM,
      category: `started`,
      name: `Rückfrage`,
      color: `#EF4444`,
    })
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(
      `actor`,
      TEAM,
      `mutate_resources`
    )
    expect(inserts[0].values).toMatchObject({
      teamId: TEAM,
      category: `started`,
      sortOrder: 3,
      builtinKey: null,
    })
    expect(result.txId).toBe(77)
  })

  it(`refuses the duplicate category`, async () => {
    await expect(
      caller.create({
        teamId: TEAM,
        category: `duplicate`,
        name: `Dupe 2`,
        color: `#EF4444`,
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`caps started at 4 statuses`, async () => {
    selectQueue.push([
      { id: uuid(21), sortOrder: 1 },
      { id: uuid(22), sortOrder: 2 },
      { id: uuid(23), sortOrder: 3 },
      { id: uuid(24), sortOrder: 4 },
    ])
    await expect(
      caller.create({
        teamId: TEAM,
        category: `started`,
        name: `Fifth`,
        color: `#EF4444`,
      })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    expect(inserts).toHaveLength(0)
  })

  it(`maps a case-insensitive name clash to CONFLICT (builtin shadowing included)`, async () => {
    selectQueue.push([], [{ id: uuid(30) }])
    await expect(
      caller.create({
        teamId: TEAM,
        category: `completed`,
        name: `done`,
        color: `#EF4444`,
      })
    ).rejects.toMatchObject({ code: `CONFLICT` })
  })
})

describe(`statuses.update`, () => {
  it(`rejects builtin rows`, async () => {
    selectQueue.push([statusRow({ builtinKey: `in_progress` })])
    await expect(
      caller.update({ teamId: TEAM, statusId: STATUS_A, name: `Renamed` })
    ).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `Built-in statuses cannot be edited or deleted`,
    })
  })

  it(`renames + recolors a custom status`, async () => {
    selectQueue.push([statusRow()], [])
    const result = await caller.update({
      teamId: TEAM,
      statusId: STATUS_A,
      name: `Blocked`,
      color: `#0EA5E9`,
    })
    expect(updates[0].set).toEqual({ name: `Blocked`, color: `#0EA5E9` })
    expect(result.txId).toBe(77)
  })
})

describe(`statuses.move`, () => {
  it(`swaps with the neighbor and re-indexes the category`, async () => {
    selectQueue.push(
      [statusRow()],
      // category in display order: builtin(1), builtin(2), custom(3)
      [
        { id: uuid(21), sortOrder: 1 },
        { id: uuid(22), sortOrder: 2 },
        { id: STATUS_A, sortOrder: 3 },
      ]
    )
    await caller.move({ teamId: TEAM, statusId: STATUS_A, direction: `up` })
    // Positions become 1..3 with the custom row and its neighbor swapped;
    // only changed rows are written.
    expect(updates.map((u) => u.set)).toEqual([
      { sortOrder: 2 },
      { sortOrder: 3 },
    ])
  })

  it(`is a no-op at the category edge`, async () => {
    selectQueue.push(
      [statusRow()],
      [
        { id: uuid(21), sortOrder: 1 },
        { id: uuid(22), sortOrder: 2 },
        { id: STATUS_A, sortOrder: 3 },
      ]
    )
    await caller.move({ teamId: TEAM, statusId: STATUS_A, direction: `down` })
    expect(updates).toHaveLength(0)
  })

  it(`moves builtins too (only edit/delete are locked)`, async () => {
    selectQueue.push(
      [statusRow({ builtinKey: `in_review`, sortOrder: 2 })],
      [
        { id: uuid(21), sortOrder: 1 },
        { id: STATUS_A, sortOrder: 2 },
      ]
    )
    await caller.move({ teamId: TEAM, statusId: STATUS_A, direction: `up` })
    expect(updates.map((u) => u.set)).toEqual([
      { sortOrder: 1 },
      { sortOrder: 2 },
    ])
  })
})

describe(`statuses.delete`, () => {
  it(`rejects builtin rows`, async () => {
    selectQueue.push([statusRow({ builtinKey: `backlog` })])
    await expect(
      caller.delete({ teamId: TEAM, statusId: STATUS_A })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`demands a replacement while issues reference the status`, async () => {
    selectQueue.push(
      [statusRow()],
      [
        { id: ISSUE_1, status: `in_progress`, duplicateOfId: null },
        { id: ISSUE_2, status: `in_progress`, duplicateOfId: null },
      ]
    )
    await expect(
      caller.delete({ teamId: TEAM, statusId: STATUS_A })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `2 issues use this status. Pick a replacement first.`,
    })
    expect(deletes).toHaveLength(0)
  })

  it(`reassigns with anchor + completedAt derivations, records events, no notifications`, async () => {
    selectQueue.push(
      [statusRow()],
      [{ id: ISSUE_1, status: `in_progress`, duplicateOfId: null }],
      // reassign target: a custom completed status → anchor `done`
      [
        statusRow({
          id: STATUS_B,
          category: `completed`,
          name: `Shipped`,
          builtinKey: null,
        }),
      ]
    )
    const result = await caller.delete({
      teamId: TEAM,
      statusId: STATUS_A,
      reassignToId: STATUS_B,
    })
    expect(updates).toHaveLength(1)
    expect(updates[0].set).toMatchObject({
      status: `done`,
      statusId: STATUS_B,
    })
    // Transition into a completed anchor stamps completedAt.
    expect(updates[0].set.completedAt).toBeInstanceOf(Date)
    expect(h.recordIssueEvent).toHaveBeenCalledTimes(1)
    expect(h.recordIssueEvent.mock.calls[0][1]).toMatchObject({
      issueId: ISSUE_1,
      type: `status_changed`,
      payload: {
        fromStatusId: STATUS_A,
        toStatusId: STATUS_B,
        fromName: `Rückfrage`,
        toName: `Shipped`,
      },
    })
    expect(deletes).toHaveLength(1)
    expect(result).toMatchObject({ reassigned: 1, reassignedToId: STATUS_B })
  })

  it(`refuses the duplicate builtin as a reassignment target`, async () => {
    selectQueue.push(
      [statusRow()],
      [{ id: ISSUE_1, status: `in_progress`, duplicateOfId: null }],
      [
        statusRow({
          id: STATUS_B,
          category: `duplicate`,
          name: `Duplicate`,
          builtinKey: `duplicate`,
        }),
      ]
    )
    await expect(
      caller.delete({
        teamId: TEAM,
        statusId: STATUS_A,
        reassignToId: STATUS_B,
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(deletes).toHaveLength(0)
  })

  it(`deletes an unreferenced status without a replacement`, async () => {
    selectQueue.push([statusRow()], [])
    const result = await caller.delete({ teamId: TEAM, statusId: STATUS_A })
    expect(deletes).toHaveLength(1)
    expect(result).toMatchObject({ reassigned: 0, reassignedToId: null })
  })
})

describe(`gating`, () => {
  it(`propagates a membership failure before any write`, async () => {
    h.resolveTeamAccess.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN` })
    )
    await expect(
      caller.create({
        teamId: TEAM,
        category: `started`,
        name: `X`,
        color: `#EF4444`,
      })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    expect(inserts).toHaveLength(0)
  })
})

// EXP-319 — per-team PR automation targets.
describe(`statuses.setPrAutomation`, () => {
  it(`pins a status row as the pr_opened target (member-gated)`, async () => {
    selectQueue.push([{ id: STATUS_A, category: `started` }])
    const result = await caller.setPrAutomation({
      teamId: TEAM,
      event: `pr_opened`,
      target: STATUS_A,
    })
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(
      `actor`,
      TEAM,
      `mutate_resources`
    )
    expect(updates).toHaveLength(1)
    expect(updates[0]!.set).toEqual({
      prOpenedStatusId: STATUS_A,
      prOpenedAutomation: true,
    })
    expect(result.txId).toBe(77)
  })

  it(`'none' disables the automation and clears the pin`, async () => {
    await caller.setPrAutomation({
      teamId: TEAM,
      event: `pr_merged`,
      target: `none`,
    })
    expect(updates[0]!.set).toEqual({
      prMergedStatusId: null,
      prMergedAutomation: false,
    })
  })

  it(`'default' resets to the builtin fallback (NULL + enabled)`, async () => {
    await caller.setPrAutomation({
      teamId: TEAM,
      event: `pr_merged`,
      target: `default`,
    })
    expect(updates[0]!.set).toEqual({
      prMergedStatusId: null,
      prMergedAutomation: true,
    })
  })

  it(`404s a status outside the team`, async () => {
    selectQueue.push([])
    await expect(
      caller.setPrAutomation({
        teamId: TEAM,
        event: `pr_opened`,
        target: STATUS_A,
      })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    expect(updates).toHaveLength(0)
  })

  it(`refuses a duplicate-category target (needs a canonical issue)`, async () => {
    selectQueue.push([{ id: STATUS_A, category: `duplicate` }])
    await expect(
      caller.setPrAutomation({
        teamId: TEAM,
        event: `pr_merged`,
        target: STATUS_A,
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(updates).toHaveLength(0)
  })
})

// EXP-711 — does a merged PR end the live coding sessions on its issues?
describe(`statuses.setEndSessionsOnMerge`, () => {
  it(`flips the team flag (member-gated)`, async () => {
    const result = await caller.setEndSessionsOnMerge({
      teamId: TEAM,
      enabled: false,
    })
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(
      `actor`,
      TEAM,
      `mutate_resources`
    )
    expect(updates).toHaveLength(1)
    expect(updates[0]!.set).toEqual({ endSessionsOnMerge: false })
    expect(result.txId).toBe(77)
  })
})
