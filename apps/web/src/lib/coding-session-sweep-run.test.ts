import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-888: the sweep ENDS stale rows on `stale-end` devices (flip first) and
// DELETES the rest for older builds. The fake db records both statements.
const calls: { op: string; values?: Record<string, unknown> }[] = []
const results: { ended: unknown[]; deleted: unknown[] } = { ended: [], deleted: [] }

vi.mock(`@/db/connection`, () => ({
  db: {
    update: () => ({
      set: (values: Record<string, unknown>) => ({
        where: () => ({
          returning: async () => {
            calls.push({ op: `update`, values })
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

import { runCodingSessionSweep } from "@/lib/coding-session-sweep"

beforeEach(() => {
  calls.length = 0
  results.ended = []
  results.deleted = []
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
})
