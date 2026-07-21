import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// Locks the REV2-3 fan-out contract of sendToUsers: ONE fcm_tokens query for
// the whole recipient set (no per-recipient SELECT), per-user relay POSTs
// carrying the recipient's userId in data, a bounded worker pool (a wedged
// relay may pin at most RELAY_CONCURRENCY of Bun's process-global fetch-pool
// slots — the same pool the Electric shape proxy long-polls through), a
// timeout signal on every POST, and error isolation (one failed POST never
// aborts the rest or throws).

const h = vi.hoisted(() => {
  // The module caches PUSH_RELAY_URL at first use — set it before import.
  process.env.PUSH_RELAY_URL = `http://relay.test`
  return {
    selectResults: [] as unknown[][],
    selectCalls: { count: 0 },
    deleteCalls: { count: 0 },
  }
})

vi.mock(`@/db/connection`, () => {
  const builder = (result: unknown[]) => {
    const b = {
      from: () => b,
      where: () => b,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      then: (res: any, rej: any) => Promise.resolve(result).then(res, rej),
    }
    return b
  }
  return {
    db: {
      select: () => {
        h.selectCalls.count += 1
        return builder(h.selectResults.shift() ?? [])
      },
      delete: () => ({
        where: () => {
          h.deleteCalls.count += 1
          return Promise.resolve([])
        },
      }),
    },
  }
})

import { sendToUsers } from "./fcm"

const fetchMock = vi.fn()

function relayOk(invalidTokens: string[] = []) {
  return {
    ok: true,
    status: 200,
    json: async () => ({ invalidTokens }),
    text: async () => ``,
  }
}

beforeEach(() => {
  h.selectResults.length = 0
  h.selectCalls.count = 0
  h.deleteCalls.count = 0
  fetchMock.mockReset()
  vi.stubGlobal(`fetch`, fetchMock)
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe(`sendToUsers (REV2-3)`, () => {
  it(`fetches all recipients' tokens in ONE query and POSTs once per user with their userId in data`, async () => {
    h.selectResults.push([
      { userId: `u1`, token: `t1a` },
      { userId: `u1`, token: `t1b` },
      { userId: `u2`, token: `t2a` },
    ])
    fetchMock.mockResolvedValue(relayOk())

    await sendToUsers([`u1`, `u2`, `u3-no-tokens`], {
      title: `Hello`,
      body: `World`,
      data: { type: `issue_comment`, issueId: `i-1` },
    })

    expect(h.selectCalls.count).toBe(1)
    expect(fetchMock).toHaveBeenCalledTimes(2)

    const bodies = fetchMock.mock.calls.map((call) =>
      JSON.parse((call[1] as RequestInit).body as string)
    )
    expect(bodies).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          tokens: [`t1a`, `t1b`],
          data: { type: `issue_comment`, issueId: `i-1`, userId: `u1` },
        }),
        expect.objectContaining({
          tokens: [`t2a`],
          data: { type: `issue_comment`, issueId: `i-1`, userId: `u2` },
        }),
      ])
    )
    // Every POST carries an abort signal so a wedged relay can't hold the
    // socket past the timeout.
    for (const call of fetchMock.mock.calls) {
      expect((call[1] as RequestInit).signal).toBeInstanceOf(AbortSignal)
    }
  })

  it(`caps in-flight relay requests at the worker-pool size`, async () => {
    h.selectResults.push(
      Array.from({ length: 20 }, (_, i) => ({
        userId: `u${i}`,
        token: `t${i}`,
      }))
    )

    let active = 0
    let peak = 0
    fetchMock.mockImplementation(async () => {
      active += 1
      peak = Math.max(peak, active)
      await new Promise((resolve) => setTimeout(resolve, 1))
      active -= 1
      return relayOk()
    })

    await sendToUsers(
      Array.from({ length: 20 }, (_, i) => `u${i}`),
      { title: `Hi`, data: {} }
    )

    expect(fetchMock).toHaveBeenCalledTimes(20)
    // The 8 workers all start before any request settles, so the peak is
    // exactly the pool size — never the full 20-request fan-out.
    expect(peak).toBe(8)
  })

  it(`collects invalid tokens across users into one prune and survives per-request failures`, async () => {
    h.selectResults.push([
      { userId: `u1`, token: `t1` },
      { userId: `u2`, token: `t2` },
      { userId: `u3`, token: `t3` },
    ])
    fetchMock
      .mockResolvedValueOnce(relayOk([`t1`]))
      .mockRejectedValueOnce(new Error(`relay wedged`))
      .mockResolvedValueOnce(relayOk([`t3`]))

    await expect(
      sendToUsers([`u1`, `u2`, `u3`], { title: `Hi`, data: {} })
    ).resolves.toBeUndefined()

    expect(fetchMock).toHaveBeenCalledTimes(3)
    expect(h.deleteCalls.count).toBe(1)
  })

  it(`skips the relay entirely when no recipient has a token`, async () => {
    h.selectResults.push([])

    await sendToUsers([`u1`], { title: `Hi`, data: {} })

    expect(fetchMock).not.toHaveBeenCalled()
    expect(h.deleteCalls.count).toBe(0)
  })

  it(`no-ops on an empty recipient list without touching the db`, async () => {
    await sendToUsers([], { title: `Hi`, data: {} })

    expect(h.selectCalls.count).toBe(0)
    expect(fetchMock).not.toHaveBeenCalled()
  })
})
