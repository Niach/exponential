import { beforeEach, describe, expect, it, vi } from "vitest"

// The sweep selects a bounded batch of TERMINAL rows past the retention
// window, then deletes exactly those ids under the same predicate. The fake
// db records both statements (and the batch limit) without a database.
const calls: { op: string; limit?: number }[] = []
const results: { due: { id: string }[]; deleted: unknown[] } = {
  due: [],
  deleted: [],
}

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({
      from: () => ({
        where: () => ({
          limit: async (limit: number) => {
            calls.push({ op: `select`, limit })
            return results.due
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

import {
  DEVICE_COMMAND_RETENTION_MS,
  DEVICE_COMMAND_TERMINAL_STATUSES,
  isDeviceCommandPurgeDue,
  runDeviceCommandSweep,
} from "@/lib/device-command-sweep"

beforeEach(() => {
  calls.length = 0
  results.due = []
  results.deleted = []
})

describe(`isDeviceCommandPurgeDue`, () => {
  const now = new Date(`2026-09-17T12:00:00Z`)
  const old = new Date(now.getTime() - DEVICE_COMMAND_RETENTION_MS - 1000)
  const fresh = new Date(now.getTime() - 60_000)

  it(`never purges a pending row, however old`, () => {
    expect(isDeviceCommandPurgeDue(`pending`, old, now)).toBe(false)
  })

  it(`purges every terminal status past the window`, () => {
    for (const status of DEVICE_COMMAND_TERMINAL_STATUSES) {
      expect(isDeviceCommandPurgeDue(status, old, now)).toBe(true)
    }
  })

  it(`keeps a terminal row inside the window`, () => {
    expect(isDeviceCommandPurgeDue(`done`, fresh, now)).toBe(false)
  })

  it(`treats the exact boundary as due`, () => {
    const boundary = new Date(now.getTime() - DEVICE_COMMAND_RETENTION_MS)
    expect(isDeviceCommandPurgeDue(`failed`, boundary, now)).toBe(true)
  })
})

describe(`runDeviceCommandSweep`, () => {
  it(`deletes the selected batch of due rows`, async () => {
    results.due = [{ id: `a` }, { id: `b` }]
    results.deleted = [{ id: `a` }, { id: `b` }]

    const result = await runDeviceCommandSweep(new Date(`2026-09-17T12:00:00Z`))

    expect(result).toEqual({ commandsDeleted: 2 })
    expect(calls.map((call) => call.op)).toEqual([`select`, `delete`])
    expect(calls[0]!.limit).toBeGreaterThan(0)
  })

  it(`skips the delete entirely when nothing is due`, async () => {
    const result = await runDeviceCommandSweep(new Date(`2026-09-17T12:00:00Z`))

    expect(result).toEqual({ commandsDeleted: 0 })
    expect(calls.map((call) => call.op)).toEqual([`select`])
  })

  it(`reports only the rows the delete actually claimed`, async () => {
    results.due = [{ id: `a` }, { id: `b` }]
    results.deleted = [{ id: `a` }]

    const result = await runDeviceCommandSweep(new Date(`2026-09-17T12:00:00Z`))

    expect(result).toEqual({ commandsDeleted: 1 })
  })
})
