import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-888: the sweep ENDS stale rows on `stale-end` devices (flip first) and
// DELETES the rest for older builds. The fake db records both statements.
const calls: { op: string; values?: Record<string, unknown> }[] = []
const results: { ended: unknown[]; deleted: unknown[] } = { ended: [], deleted: [] }
// Set to make the stale-end flip throw (EXP-888: the flip is the optional
// half — a failure must degrade to the legacy delete, never abort the pass).
let updateThrows = false
const h = vi.hoisted(() => ({
  notifyParentOfChildEnd: vi.fn(async () => ({ delivered: true })),
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    update: () => ({
      set: (values: Record<string, unknown>) => ({
        where: () => ({
          returning: async () => {
            calls.push({ op: `update`, values })
            if (updateThrows) throw new Error(`lock timeout`)
            return results.ended
          },
        }),
      }),
    }),
    delete: () => ({
      where: () => ({
        returning: async () => {
          calls.push({ op: `delete` })
          return results.deleted
        },
      }),
    }),
  },
}))
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
  it(`ends cap-aware rows as stale before deleting the legacy remainder`, async () => {
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
  })

  it(`degrades to the legacy delete when the stale-end flip fails`, async () => {
    updateThrows = true
    results.deleted = [{ id: `c` }, { id: `d` }]

    const result = await runCodingSessionSweep(new Date())

    // The pass completes: the delete still ran and the counts report it.
    expect(result).toEqual({ sessionsEnded: 0, sessionsDeleted: 2 })
    expect(calls.map((call) => call.op)).toEqual([`update`, `delete`])
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
