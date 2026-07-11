import { beforeEach, describe, expect, it, vi } from "vitest"
import { is, Param, SQL } from "drizzle-orm"

// EXP-56 Phase 2 — release-level PR lifecycle writers (DB side; the pure
// transition guard stays covered in pr-sync.test.ts). pr-sync.ts binds the
// module-scope `db`, so the fake db lives inside the `@/db/connection` mock:
// `select()` shifts pre-seeded rows off a FIFO queue (thenable chain, mirrors
// releases.test.ts), `update()` records table + payload + where clause,
// `transaction()` hands the callback the same fake, and `execute` serves
// generateTxId's pg_current_xact_id probe.

const h = vi.hoisted(() => {
  const selectQueue: unknown[][] = []
  const updates: {
    table: unknown
    set: Record<string, unknown>
    where: unknown
  }[] = []

  function selectChain(): Promise<unknown[]> & Record<string, () => unknown> {
    const p = Promise.resolve(
      selectQueue.shift() ?? []
    ) as Promise<unknown[]> & Record<string, () => unknown>
    for (const m of [`from`, `where`, `innerJoin`, `limit`]) {
      p[m] = () => p
    }
    return p
  }

  const fakeDb = {
    select: () => selectChain(),
    update: (table: unknown) => ({
      set: (values: Record<string, unknown>) => ({
        where: (whereArg: unknown) => {
          updates.push({ table, set: values, where: whereArg })
          return Promise.resolve()
        },
      }),
    }),
    // generateTxId's `SELECT pg_current_xact_id()` probe.
    execute: async () => ({ rows: [{ txid: `42` }] }),
    transaction: async (
      fn: (tx: unknown) => Promise<unknown>
    ): Promise<unknown> => fn(fakeDb),
  }

  return { selectQueue, updates, fakeDb }
})

vi.mock(`@/db/connection`, () => ({ db: h.fakeDb }))
// lib/trpc.ts (generateTxId's home) imports auth at module scope; the issue
// writers' event/notify deps would otherwise pull in email/fcm chains.
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/integrations/activity`, () => ({ recordIssueEvent: vi.fn() }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetPrNotify: vi.fn(),
}))

import {
  applyReleasePrClosedState,
  applyReleasePrMergeState,
  applyReleasePrReopenedState,
  findReleaseIdByPrUrl,
} from "@/lib/integrations/pr-sync"
import { releases } from "@/db/schema"

const RELEASE_ID = `22222222-2222-4222-8222-222222222222`
const PR_URL = `https://github.com/org/repo/pull/7`
const OTHER_PR_URL = `https://github.com/org/repo/pull/8`
const MERGED_AT = new Date(`2026-07-11T10:00:00.000Z`)

// Digs the bound values out of a drizzle where-clause (eq produces SQL nodes
// whose Params carry the id) — asserts the update targeted the right release.
function collectParams(node: unknown, out: unknown[] = []): unknown[] {
  if (is(node, Param)) {
    out.push(node.value)
  } else if (is(node, SQL)) {
    for (const chunk of node.queryChunks) collectParams(chunk, out)
  } else if (Array.isArray(node)) {
    for (const item of node) collectParams(item, out)
  }
  return out
}

beforeEach(() => {
  h.selectQueue.length = 0
  h.updates.length = 0
})

describe(`findReleaseIdByPrUrl`, () => {
  it(`returns the release id on an exact pr_url hit`, async () => {
    h.selectQueue.push([{ id: RELEASE_ID }])

    await expect(findReleaseIdByPrUrl(PR_URL)).resolves.toBe(RELEASE_ID)
  })

  it(`returns null when no release carries that pr_url`, async () => {
    h.selectQueue.push([])

    await expect(findReleaseIdByPrUrl(PR_URL)).resolves.toBeNull()
  })
})

describe(`applyReleasePrMergeState`, () => {
  it(`open→merged writes prState/prMergedAt and AUTO-SHIPS (shippedAt = mergedAt) when unshipped`, async () => {
    h.selectQueue.push([{ prState: `open`, prUrl: PR_URL, shippedAt: null }])

    await applyReleasePrMergeState({
      releaseId: RELEASE_ID,
      prUrl: PR_URL,
      mergedAt: MERGED_AT,
    })

    expect(h.updates).toHaveLength(1)
    expect(h.updates[0]!.table).toBe(releases)
    expect(h.updates[0]!.set).toEqual({
      prState: `merged`,
      prMergedAt: MERGED_AT,
      shippedAt: MERGED_AT,
    })
    expect(collectParams(h.updates[0]!.where)).toEqual([RELEASE_ID])
  })

  it(`preserves an existing shippedAt (an already-shipped release keeps its ship date)`, async () => {
    const shippedAt = new Date(`2026-07-01T00:00:00.000Z`)
    h.selectQueue.push([{ prState: `open`, prUrl: PR_URL, shippedAt }])

    await applyReleasePrMergeState({
      releaseId: RELEASE_ID,
      prUrl: PR_URL,
      mergedAt: MERGED_AT,
    })

    expect(h.updates).toHaveLength(1)
    expect(h.updates[0]!.set.prState).toBe(`merged`)
    expect(h.updates[0]!.set.prMergedAt).toBe(MERGED_AT)
    expect(h.updates[0]!.set.shippedAt).toBe(shippedAt)
  })

  it(`defaults mergedAt to now and ships with the same stamp when the payload omits it`, async () => {
    h.selectQueue.push([{ prState: `open`, prUrl: PR_URL, shippedAt: null }])

    await applyReleasePrMergeState({ releaseId: RELEASE_ID, prUrl: PR_URL })

    expect(h.updates).toHaveLength(1)
    expect(h.updates[0]!.set.prMergedAt).toBeInstanceOf(Date)
    expect(h.updates[0]!.set.shippedAt).toBe(h.updates[0]!.set.prMergedAt)
  })

  it(`allows merging a closed-then-reopened release PR (closed → merged)`, async () => {
    h.selectQueue.push([{ prState: `closed`, prUrl: PR_URL, shippedAt: null }])

    await applyReleasePrMergeState({
      releaseId: RELEASE_ID,
      prUrl: PR_URL,
      mergedAt: MERGED_AT,
    })

    expect(h.updates).toHaveLength(1)
    expect(h.updates[0]!.set.prState).toBe(`merged`)
  })

  it(`is idempotent when already merged (webhook re-delivery never re-writes)`, async () => {
    h.selectQueue.push([
      { prState: `merged`, prUrl: PR_URL, shippedAt: MERGED_AT },
    ])

    await applyReleasePrMergeState({
      releaseId: RELEASE_ID,
      prUrl: PR_URL,
      mergedAt: new Date(),
    })

    expect(h.updates).toHaveLength(0)
  })

  it(`refuses when the stored prUrl differs from the transition prUrl`, async () => {
    h.selectQueue.push([{ prState: `open`, prUrl: PR_URL, shippedAt: null }])

    await applyReleasePrMergeState({
      releaseId: RELEASE_ID,
      prUrl: OTHER_PR_URL,
      mergedAt: MERGED_AT,
    })

    expect(h.updates).toHaveLength(0)
  })

  it(`no-ops for an unknown release`, async () => {
    h.selectQueue.push([])

    await applyReleasePrMergeState({
      releaseId: RELEASE_ID,
      prUrl: PR_URL,
      mergedAt: MERGED_AT,
    })

    expect(h.updates).toHaveLength(0)
  })
})

describe(`applyReleasePrClosedState`, () => {
  it(`flips open→closed for the linked PR`, async () => {
    h.selectQueue.push([{ prState: `open`, prUrl: PR_URL }])

    await applyReleasePrClosedState({ releaseId: RELEASE_ID, prUrl: PR_URL })

    expect(h.updates).toHaveLength(1)
    expect(h.updates[0]!.table).toBe(releases)
    expect(h.updates[0]!.set).toEqual({ prState: `closed` })
    expect(collectParams(h.updates[0]!.where)).toEqual([RELEASE_ID])
  })

  it(`refuses to close a merged release PR`, async () => {
    h.selectQueue.push([{ prState: `merged`, prUrl: PR_URL }])

    await applyReleasePrClosedState({ releaseId: RELEASE_ID, prUrl: PR_URL })

    expect(h.updates).toHaveLength(0)
  })

  it(`refuses a foreign PR URL`, async () => {
    h.selectQueue.push([{ prState: `open`, prUrl: PR_URL }])

    await applyReleasePrClosedState({
      releaseId: RELEASE_ID,
      prUrl: OTHER_PR_URL,
    })

    expect(h.updates).toHaveLength(0)
  })

  it(`no-ops for an unknown release`, async () => {
    h.selectQueue.push([])

    await applyReleasePrClosedState({ releaseId: RELEASE_ID, prUrl: PR_URL })

    expect(h.updates).toHaveLength(0)
  })
})

describe(`applyReleasePrReopenedState`, () => {
  it(`heals closed→open for the linked PR`, async () => {
    h.selectQueue.push([{ prState: `closed`, prUrl: PR_URL }])

    await applyReleasePrReopenedState({ releaseId: RELEASE_ID, prUrl: PR_URL })

    expect(h.updates).toHaveLength(1)
    expect(h.updates[0]!.table).toBe(releases)
    expect(h.updates[0]!.set).toEqual({ prState: `open` })
    expect(collectParams(h.updates[0]!.where)).toEqual([RELEASE_ID])
  })

  it(`refuses to reopen an already-open release PR`, async () => {
    h.selectQueue.push([{ prState: `open`, prUrl: PR_URL }])

    await applyReleasePrReopenedState({ releaseId: RELEASE_ID, prUrl: PR_URL })

    expect(h.updates).toHaveLength(0)
  })

  it(`refuses to reopen a merged release PR`, async () => {
    h.selectQueue.push([{ prState: `merged`, prUrl: PR_URL }])

    await applyReleasePrReopenedState({ releaseId: RELEASE_ID, prUrl: PR_URL })

    expect(h.updates).toHaveLength(0)
  })

  it(`refuses a foreign PR URL`, async () => {
    h.selectQueue.push([{ prState: `closed`, prUrl: PR_URL }])

    await applyReleasePrReopenedState({
      releaseId: RELEASE_ID,
      prUrl: OTHER_PR_URL,
    })

    expect(h.updates).toHaveLength(0)
  })
})
