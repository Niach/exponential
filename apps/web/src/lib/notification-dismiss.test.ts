import { beforeEach, describe, expect, it, vi } from "vitest"

// #EXP-1815 (notification-delivery.json CLEARING): the rows a read write
// turned read go out as ONE silent push to the reader's devices.
const { selectResults } = vi.hoisted(() => ({ selectResults: [] as unknown[][] }))

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => {
      const p = Promise.resolve(selectResults.shift() ?? []) as Promise<unknown[]> & Record<string, () => unknown>
      for (const m of [`from`, `where`]) p[m] = () => p
      return p
    },
  },
}))
vi.mock(`@/lib/integrations/fcm`, () => ({ sendToUsers: vi.fn(async () => {}) }))

import { sendToUsers } from "@/lib/integrations/fcm"
import { dismissReadNotifications } from "./notification-dismiss"

const row = (id: string, extra: Partial<{ issueId: string; sessionId: string; type: string }> = {}) => ({
  id,
  type: extra.type ?? `agent_message`,
  issueId: extra.issueId ?? null,
  sessionId: extra.sessionId ?? null,
  deviceId: null,
})

beforeEach(() => {
  selectResults.length = 0
  vi.mocked(sendToUsers).mockClear()
})

describe(`dismissReadNotifications`, () => {
  it(`nothing turned read: no push`, async () => {
    await dismissReadNotifications({ userId: `u1`, rows: [] })
    expect(sendToUsers).not.toHaveBeenCalled()
  })

  it(`a few rows: one silent push naming their ids`, async () => {
    selectResults.push([{ id: `s1`, rootId: `r1` }])
    await dismissReadNotifications({ userId: `u1`, rows: [row(`n1`, { sessionId: `s1` }), row(`n2`, { issueId: `i1` })] })
    expect(sendToUsers).toHaveBeenCalledWith([{ userId: `u1` }], {
      title: ``,
      data: { type: `dismiss`, ids: `n1,n2` },
      silent: true,
    })
  })

  it(`many rows: the distinct threads, a session row's chat = its root`, async () => {
    selectResults.push([{ id: `s1`, rootId: `r1` }])
    const rows = Array.from({ length: 61 }, (_, i) => row(`n${i}`, i % 2 ? { sessionId: `s1` } : { issueId: `i1` }))
    await dismissReadNotifications({ userId: `u1`, rows })
    expect(vi.mocked(sendToUsers).mock.calls[0]![1].data).toEqual({ type: `dismiss`, threads: `issue:i1,session:r1` })
  })

  it(`one team's mark all read: all of that team`, async () => {
    await dismissReadNotifications({ userId: `u1`, rows: [row(`n1`)], all: true, teamId: `t1` })
    expect(vi.mocked(sendToUsers).mock.calls[0]![1].data).toEqual({ type: `dismiss`, all: `1`, teamId: `t1` })
  })
})
