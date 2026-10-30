import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// Locks the REV2-3 fan-out contract of sendToUsers: ONE fcm_tokens query for
// the whole recipient set (no per-recipient SELECT), per-user relay POSTs
// carrying the recipient's userId in data, a bounded worker pool (a wedged
// relay may pin at most RELAY_CONCURRENCY of Bun's process-global fetch-pool
// slots — the same pool the Electric shape proxy long-polls through), a
// timeout signal on every POST, and error isolation (one failed POST never
// aborts the rest or throws).
//
// EXP-264 extends recipients from bare user ids to `{ userId, data? }`: the
// per-recipient data is merged OVER the shared payload data, and the userId is
// written LAST so nothing can clobber the routing key.

const h = vi.hoisted(() => {
  // The module caches PUSH_RELAY_URL at first use — set it before import.
  process.env.PUSH_RELAY_URL = `http://relay.test`
  return {
    selectResults: [] as unknown[][],
    selectCalls: { count: 0 },
    deleteCalls: { count: 0 },
    deleteWheres: [] as unknown[],
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
        where: (cond: unknown) => {
          h.deleteCalls.count += 1
          h.deleteWheres.push(cond)
          return Promise.resolve([])
        },
      }),
    },
  }
})

import { MAX_SEALED_BYTES, sealedPlaintext, sendToUsers } from "./fcm"
import { pushOpen, pushPublicKey } from "@/lib/push-seal"

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
  h.deleteWheres.length = 0
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

    await sendToUsers(
      [{ userId: `u1` }, { userId: `u2` }, { userId: `u3-no-tokens` }],
      {
        title: `Hello`,
        body: `World`,
        data: { type: `issue_comment`, issueId: `i-1` },
      }
    )

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
      Array.from({ length: 20 }, (_, i) => ({ userId: `u${i}` })),
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
      sendToUsers([{ userId: `u1` }, { userId: `u2` }, { userId: `u3` }], {
        title: `Hi`,
        data: {},
      })
    ).resolves.toBeUndefined()

    expect(fetchMock).toHaveBeenCalledTimes(3)
    expect(h.deleteCalls.count).toBe(1)
  })

  it(`merges per-recipient data over the shared payload, userId last`, async () => {
    h.selectResults.push([
      { userId: `u1`, token: `t1` },
      { userId: `u2`, token: `t2` },
    ])
    fetchMock.mockResolvedValue(relayOk())

    await sendToUsers(
      [
        { userId: `u1`, data: { notificationId: `n-1` } },
        // A recipient trying to override the routing key must lose: the
        // userId is written after the merge.
        { userId: `u2`, data: { notificationId: `n-2`, userId: `spoofed` } },
      ],
      { title: `Hi`, data: { type: `issue_comment`, issueId: `i-1` } }
    )

    const dataByToken = new Map(
      fetchMock.mock.calls.map((call) => {
        const body = JSON.parse((call[1] as RequestInit).body as string)
        return [body.tokens[0], body.data]
      })
    )
    expect(dataByToken.get(`t1`)).toEqual({
      type: `issue_comment`,
      issueId: `i-1`,
      notificationId: `n-1`,
      userId: `u1`,
    })
    expect(dataByToken.get(`t2`)).toEqual({
      type: `issue_comment`,
      issueId: `i-1`,
      notificationId: `n-2`,
      userId: `u2`,
    })
  })

  it(`skips the relay entirely when no recipient has a token`, async () => {
    h.selectResults.push([])

    await sendToUsers([{ userId: `u1` }], { title: `Hi`, data: {} })

    expect(fetchMock).not.toHaveBeenCalled()
    expect(h.deleteCalls.count).toBe(0)
  })

  it(`no-ops on an empty recipient list without touching the db`, async () => {
    await sendToUsers([], { title: `Hi`, data: {} })

    expect(h.selectCalls.count).toBe(0)
    expect(fetchMock).not.toHaveBeenCalled()
  })
})

// EXP-1428: a row carrying a relay capability + push public key is pushed
// sealed to POST /push (one request per device, Bearer capability); rows
// without one keep the legacy /send shim exactly as above.
describe(`sendToUsers sealed /push (EXP-1428)`, () => {
  const secret = new Uint8Array(Array.from({ length: 32 }, (_, i) => i + 7))
  const pub = Buffer.from(pushPublicKey(secret)).toString(`base64url`)

  function openBody(call: unknown[]) {
    const body = JSON.parse((call[1] as RequestInit).body as string)
    const plain = pushOpen(secret, new Uint8Array(Buffer.from(body.sealed, `base64url`)))
    return { body, payload: JSON.parse(Buffer.from(plain).toString(`utf8`)) }
  }

  it(`routes capability rows to /push sealed and legacy rows to /send`, async () => {
    h.selectResults.push([
      { userId: `u1`, token: `t1`, relayCapability: `cap_a`, pushPublicKey: pub },
      { userId: `u1`, token: `t1-old`, relayCapability: null, pushPublicKey: null },
      { userId: `u2`, token: `t2`, relayCapability: null, pushPublicKey: null },
    ])
    fetchMock.mockImplementation(async (url: string) =>
      url.endsWith(`/push`)
        ? { ok: true, status: 202, json: async () => ({ delivered: true }), text: async () => `` }
        : relayOk()
    )

    await sendToUsers(
      [{ userId: `u1`, data: { notificationId: `n-1` } }, { userId: `u2` }],
      {
        title: `Hello`,
        body: `World`,
        data: { type: `issue_comment`, issueId: `i-1` },
        collapseId: `issue-i-1`,
      }
    )

    const calls = fetchMock.mock.calls
    const push = calls.filter((c) => (c[0] as string) === `http://relay.test/push`)
    const send = calls.filter((c) => (c[0] as string) === `http://relay.test/send`)
    expect(push).toHaveLength(1)
    expect(send).toHaveLength(2)

    const init = push[0][1] as RequestInit
    expect((init.headers as Record<string, string>).Authorization).toBe(`Bearer cap_a`)
    expect(init.signal).toBeInstanceOf(AbortSignal)
    const { body, payload } = openBody(push[0])
    expect(body.collapseId).toBe(`issue-i-1`)
    expect(payload).toEqual({
      title: `Hello`,
      body: `World`,
      data: { type: `issue_comment`, issueId: `i-1`, notificationId: `n-1`, userId: `u1` },
    })
    // The sealed row is never part of a /send token batch.
    const sentTokens = send.flatMap(
      (c) => JSON.parse((c[1] as RequestInit).body as string).tokens
    )
    expect(sentTokens.sort()).toEqual([`t1-old`, `t2`])
    expect(h.deleteCalls.count).toBe(0)
  })

  it(`deletes rows whose install is gone (410) or capability revoked (401)`, async () => {
    h.selectResults.push([
      { userId: `u1`, token: `t1`, relayCapability: `cap_gone`, pushPublicKey: pub },
      { userId: `u2`, token: `t2`, relayCapability: `cap_revoked`, pushPublicKey: pub },
      { userId: `u3`, token: `t3`, relayCapability: `cap_busy`, pushPublicKey: pub },
    ])
    const status: Record<string, number> = {
      "Bearer cap_gone": 410,
      "Bearer cap_revoked": 401,
      "Bearer cap_busy": 429,
    }
    fetchMock.mockImplementation(async (_url: string, init: RequestInit) => {
      const code = status[(init.headers as Record<string, string>).Authorization]
      return { ok: false, status: code, json: async () => ({}), text: async () => `x` }
    })

    await sendToUsers([{ userId: `u1` }, { userId: `u2` }, { userId: `u3` }], {
      title: `Hi`,
      data: {},
    })

    expect(fetchMock).toHaveBeenCalledTimes(3)
    expect(h.deleteCalls.count).toBe(1)
    const where = JSON.stringify(h.deleteWheres[0], (_k, v) =>
      typeof v === `object` && v && `table` in v ? undefined : v
    )
    expect(where).toContain(`cap_gone`)
    expect(where).toContain(`cap_revoked`)
    expect(where).not.toContain(`cap_busy`)
  })
})

describe(`sealedPlaintext budget`, () => {
  const sealedSize = (p: Uint8Array) => p.length + 49

  it(`passes a small payload through untouched`, () => {
    const p = sealedPlaintext(`T`, `B`, { userId: `u`, x: `y` })
    expect(JSON.parse(Buffer.from(p).toString())).toEqual({
      title: `T`,
      body: `B`,
      data: { userId: `u`, x: `y` },
    })
  })

  it(`truncates the body first, keeping every data key`, () => {
    const data = { userId: `u`, notificationId: `n`, issueId: `i` }
    const p = sealedPlaintext(`T`, `é`.repeat(4000), data)
    const json = JSON.parse(Buffer.from(p).toString())
    expect(sealedSize(p)).toBeLessThanOrEqual(MAX_SEALED_BYTES)
    expect(json.data).toEqual(data)
    expect(json.body.endsWith(`…`)).toBe(true)
    expect(json.body.length).toBeGreaterThan(1000)
  })

  it(`then drops non-essential data keys, keeping userId / notificationId / type`, () => {
    const p = sealedPlaintext(`T`, `body`, {
      type: `issue_comment`,
      notificationId: `n`,
      userId: `u`,
      huge: `x`.repeat(5000),
    })
    const json = JSON.parse(Buffer.from(p).toString())
    expect(sealedSize(p)).toBeLessThanOrEqual(MAX_SEALED_BYTES)
    expect(json.data).toEqual({ type: `issue_comment`, notificationId: `n`, userId: `u` })
    expect(json.body).toBe(`body`)
  })
})
