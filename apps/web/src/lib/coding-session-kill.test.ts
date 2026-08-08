import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-445: the unshare/deletion kill fan-out. The helper runs one update
// inside a transaction and then relays a best-effort kill per ended row, so a
// structural fake `tx` (recording the set values and the where clause) plus
// mocked relay helpers is enough — the where clause is asserted by SHAPE,
// since a fake db cannot execute it.
const h = vi.hoisted(() => ({
  updates: [] as { set: Record<string, unknown>; where: unknown }[],
  returning: [] as { id: string }[],
  getSteerRelayConfig: vi.fn(),
  relayPostKill: vi.fn(async () => ({ delivered: true })),
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    transaction: async (fn: (tx: unknown) => Promise<unknown>) =>
      fn({
        update: () => ({
          set: (set: Record<string, unknown>) => ({
            where: (where: unknown) => ({
              returning: async () => {
                h.updates.push({ set, where })
                return h.returning
              },
            }),
          }),
        }),
      }),
  },
}))
vi.mock(`@/lib/trpc`, () => ({ generateTxId: async () => `1` }))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: h.getSteerRelayConfig,
  relayPostKill: h.relayPostKill,
}))

import {
  endForeignHostedSessions,
  relayKillSessionsBestEffort,
} from "@/lib/coding-session-kill"

const HOST = `host-user`
const TEAM = `11111111-1111-4111-8111-111111111111`

// Flatten a drizzle condition into its column names + bound values, in order:
// eq(a.b, `x`) ⇒ [`col:b`, `x`].
function whereShape(cond: unknown, out: unknown[] = []): unknown[] {
  if (!cond || typeof cond !== `object`) return out
  if (Array.isArray(cond)) {
    for (const child of cond) whereShape(child, out)
    return out
  }
  const rec = cond as Record<string, unknown>
  if (Array.isArray(rec.queryChunks)) return whereShape(rec.queryChunks, out)
  if (`value` in rec && `encoder` in rec) {
    out.push(rec.value)
    return out
  }
  if (typeof rec.name === `string` && rec.table) {
    out.push(`col:${rec.name}`)
    return out
  }
  return out
}

beforeEach(() => {
  h.updates.length = 0
  h.returning = []
  vi.clearAllMocks()
  h.getSteerRelayConfig.mockReturnValue({ url: `ws://relay`, secret: `s` })
  h.relayPostKill.mockResolvedValue({ delivered: true })
})

describe(`endForeignHostedSessions`, () => {
  it(`ends only live foreign-requester rows on this host for the team, and relays one kill each`, async () => {
    h.returning = [{ id: `sess-1` }, { id: `sess-2` }]

    const ids = await endForeignHostedSessions(HOST, TEAM)

    expect(ids).toEqual([`sess-1`, `sess-2`])
    expect(h.updates).toHaveLength(1)
    expect(h.updates[0]!.set).toMatchObject({
      status: `ended`,
      endedAt: expect.any(Date),
      updatedAt: expect.any(Date),
    })
    // Hosted BY this user, requested by SOMEONE ELSE, in the unshared team,
    // and still live (EXP-358 keeps in_review/merged steerable).
    expect(whereShape(h.updates[0]!.where)).toEqual([
      `col:host_user_id`,
      HOST,
      `col:user_id`,
      HOST,
      `col:team_id`,
      TEAM,
      `col:status`,
      `running`,
      `in_review`,
      `merged`,
    ])
    expect(h.relayPostKill).toHaveBeenCalledTimes(2)
    expect(h.relayPostKill).toHaveBeenCalledWith(expect.anything(), `sess-1`)
    expect(h.relayPostKill).toHaveBeenCalledWith(expect.anything(), `sess-2`)
  })

  it(`relays nothing when no row matched`, async () => {
    const ids = await endForeignHostedSessions(HOST, TEAM)

    expect(ids).toEqual([])
    expect(h.updates).toHaveLength(1)
    expect(h.relayPostKill).not.toHaveBeenCalled()
  })

  it(`still flips the rows when the relay is unconfigured`, async () => {
    h.getSteerRelayConfig.mockReturnValue(null)
    h.returning = [{ id: `sess-1` }]

    const ids = await endForeignHostedSessions(HOST, TEAM)

    expect(ids).toEqual([`sess-1`])
    expect(h.relayPostKill).not.toHaveBeenCalled()
  })
})

describe(`relayKillSessionsBestEffort`, () => {
  it(`is a no-op on an empty list — never even asks for the relay config`, async () => {
    await relayKillSessionsBestEffort([])

    expect(h.getSteerRelayConfig).not.toHaveBeenCalled()
    expect(h.relayPostKill).not.toHaveBeenCalled()
  })

  it(`kills one session per id when the relay is configured`, async () => {
    await relayKillSessionsBestEffort([`a`, `b`, `c`])

    expect(h.relayPostKill).toHaveBeenCalledTimes(3)
  })

  it(`does nothing when the relay is unconfigured`, async () => {
    h.getSteerRelayConfig.mockReturnValue(null)

    await relayKillSessionsBestEffort([`a`])

    expect(h.relayPostKill).not.toHaveBeenCalled()
  })
})
