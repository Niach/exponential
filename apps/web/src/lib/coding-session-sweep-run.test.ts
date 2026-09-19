import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-888: the sweep ENDS stale rows on `stale-end` devices (flip first);
// EXP-972: it DELETES only stale rows with NO devices row left to consult.
// The fake db records both statements and their where clauses.
const calls: { op: string; values?: Record<string, unknown>; where?: unknown }[] = []
const results: { ended: unknown[]; deleted: unknown[] } = { ended: [], deleted: [] }
// Set to make the stale-end flip throw (a failure must never abort the pass,
// and must never widen the delete).
let updateThrows = false
const h = vi.hoisted(() => ({
  notifyParentOfChildEnd: vi.fn(async () => ({ delivered: true })),
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    update: () => ({
      set: (values: Record<string, unknown>) => ({
        where: (where: unknown) => ({
          returning: async () => {
            calls.push({ op: `update`, values, where })
            if (updateThrows) throw new Error(`lock timeout`)
            return results.ended
          },
        }),
      }),
    }),
    delete: () => ({
      where: (where: unknown) => ({
        returning: async () => {
          calls.push({ op: `delete`, where })
          return results.deleted
        },
      }),
    }),
  },
}))

// The literal text of a drizzle condition (StringChunks + nested fragments;
// bound params and column refs are skipped), enough to tell which predicate
// a statement carries.
function sqlText(node: unknown): string {
  if (!node || typeof node !== `object`) return ``
  const rec = node as Record<string, unknown>
  if (Array.isArray(rec.queryChunks)) return rec.queryChunks.map(sqlText).join(``)
  if (Array.isArray(rec.value)) return rec.value.join(``)
  return ``
}
vi.mock(`@/lib/metrics/registry`, () => ({ reportSchedulerRun: vi.fn() }))
vi.mock(`@/lib/steer-child-messages`, () => ({
  notifyParentOfChildEnd: h.notifyParentOfChildEnd,
}))

import { runCodingSessionSweep } from "@/lib/coding-session-sweep"

beforeEach(() => {
  calls.length = 0
  results.ended = []
  results.deleted = []
  updateThrows = false
  h.notifyParentOfChildEnd.mockClear()
})

describe(`runCodingSessionSweep`, () => {
  it(`ends cap-aware rows as stale before deleting the device-less orphans`, async () => {
    results.ended = [{ id: `a` }, { id: `b` }]
    results.deleted = [{ id: `c` }]
    const now = new Date(`2026-09-16T12:00:00Z`)

    const result = await runCodingSessionSweep(now)

    expect(result).toEqual({ sessionsEnded: 2, sessionsDeleted: 1 })
    expect(calls.map((call) => call.op)).toEqual([`update`, `delete`])
    expect(calls[0]!.values).toMatchObject({
      status: `ended`,
      endedBy: `stale`,
      endedAt: now,
      agentBusy: false,
    })
    // The flip asks the host device for the cap; the delete asks for the
    // ABSENCE of any host device row — a row whose device exists is never
    // deleted, only flipped.
    expect(sqlText(calls[0]!.where)).toMatch(/exists \(/)
    expect(sqlText(calls[0]!.where)).toMatch(/\? /)
    expect(sqlText(calls[1]!.where)).toMatch(/not exists \(/)
    expect(sqlText(calls[1]!.where)).not.toMatch(/\? /)
  })

  it(`keeps the delete confined to orphans when the stale-end flip fails`, async () => {
    updateThrows = true
    results.deleted = [{ id: `c` }, { id: `d` }]

    const result = await runCodingSessionSweep(new Date())

    // The pass completes: the orphan delete still ran and the counts report
    // it. The rows the flip missed wait for the next pass — the failure never
    // widens the delete onto them.
    expect(result).toEqual({ sessionsEnded: 0, sessionsDeleted: 2 })
    expect(calls.map((call) => call.op)).toEqual([`update`, `delete`])
    expect(sqlText(calls[1]!.where)).toMatch(/not exists \(/)
    expect(h.notifyParentOfChildEnd).not.toHaveBeenCalled()
  })

  it(`tells a live parent about each swept CHILD (EXP-700)`, async () => {
    results.ended = [
      { id: `child`, parentSessionId: `parent` },
      { id: `lone`, parentSessionId: null },
    ]

    await runCodingSessionSweep(new Date())

    // Only the row with a parent is looked up — the helper itself no-ops on
    // anything that is not agent-started with a live parent.
    expect(h.notifyParentOfChildEnd).toHaveBeenCalledTimes(1)
    expect(h.notifyParentOfChildEnd).toHaveBeenCalledWith(
      expect.anything(),
      `child`,
      { summary: null, endedBy: `stale` }
    )
  })
})
