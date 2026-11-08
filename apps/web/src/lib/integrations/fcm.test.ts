import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// Locks the REV2-3 fan-out contract of sendToUsers: ONE fcm_tokens query for
// the whole recipient set (no per-recipient SELECT), one sealed relay POST
// /push per device carrying the recipient's userId in data, a bounded worker
// pool (a wedged relay may pin at most RELAY_CONCURRENCY of Bun's
// process-global fetch-pool slots — the same pool the Electric shape proxy
// long-polls through), a timeout signal on every POST, and error isolation
// (one failed POST never aborts the rest or throws).
//
// EXP-264 extends recipients from bare user ids to `{ userId, data? }`: the
// per-recipient data is merged OVER the shared payload data, and the userId is
// written LAST so nothing can clobber the routing key.
//
// NEW-14: only rows with a relay capability + push public key are reachable;
// the bare-token /send shim is gone.

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

const secret = new Uint8Array(Array.from({ length: 32 }, (_, i) => i + 7))
const pub = Buffer.from(pushPublicKey(secret)).toString(`base64url`)

/** A device row as the fan-out query returns it. */
function row(userId: string, capability: string) {
  return { userId, relayCapability: capability, pushPublicKey: pub }
}

function relayOk() {
  return { ok: true, status: 202, json: async () => ({ delivered: true }), text: async () => `` }
}

function openBody(call: unknown[]) {
  const body = JSON.parse((call[1] as RequestInit).body as string)
  const plain = pushOpen(secret, new Uint8Array(Buffer.from(body.sealed, `base64url`)))
  return { body, payload: JSON.parse(Buffer.from(plain).toString(`utf8`)) }
}

function capabilityOf(call: unknown[]): string {
  return ((call[1] as RequestInit).headers as Record<string, string>).Authorization
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
  it(`fetches all recipients' devices in ONE query and POSTs once per device with their userId in data`, async () => {
    h.selectResults.push([row(`u1`, `cap_1a`), row(`u1`, `cap_1b`), row(`u2`, `cap_2a`)])
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
    expect(fetchMock).toHaveBeenCalledTimes(3)

    const dataByCapability = new Map(
      fetchMock.mock.calls.map((call) => [capabilityOf(call), openBody(call).payload.data])
    )
    expect(dataByCapability.get(`Bearer cap_1a`)).toEqual({ type: `issue_comment`, issueId: `i-1`, userId: `u1` })
    expect(dataByCapability.get(`Bearer cap_1b`)).toEqual({ type: `issue_comment`, issueId: `i-1`, userId: `u1` })
    expect(dataByCapability.get(`Bearer cap_2a`)).toEqual({ type: `issue_comment`, issueId: `i-1`, userId: `u2` })
    // Every POST goes to /push and carries an abort signal so a wedged relay
    // can't hold the socket past the timeout.
    for (const call of fetchMock.mock.calls) {
      expect(call[0]).toBe(`http://relay.test/push`)
      expect((call[1] as RequestInit).signal).toBeInstanceOf(AbortSignal)
    }
  })

  it(`caps in-flight relay requests at the worker-pool size`, async () => {
    h.selectResults.push(Array.from({ length: 20 }, (_, i) => row(`u${i}`, `cap_${i}`)))

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

  it(`survives per-request failures without throwing or pruning`, async () => {
    h.selectResults.push([row(`u1`, `cap_1`), row(`u2`, `cap_2`), row(`u3`, `cap_3`)])
    fetchMock
      .mockResolvedValueOnce(relayOk())
      .mockRejectedValueOnce(new Error(`relay wedged`))
      .mockResolvedValueOnce({ ok: false, status: 500, json: async () => ({}), text: async () => `x` })

    await expect(
      sendToUsers([{ userId: `u1` }, { userId: `u2` }, { userId: `u3` }], {
        title: `Hi`,
        data: {},
      })
    ).resolves.toBeUndefined()

    expect(fetchMock).toHaveBeenCalledTimes(3)
    expect(h.deleteCalls.count).toBe(0)
  })

  it(`merges per-recipient data over the shared payload, userId last`, async () => {
    h.selectResults.push([row(`u1`, `cap_1`), row(`u2`, `cap_2`)])
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

    const dataByCapability = new Map(
      fetchMock.mock.calls.map((call) => [capabilityOf(call), openBody(call).payload.data])
    )
    expect(dataByCapability.get(`Bearer cap_1`)).toEqual({
      type: `issue_comment`,
      issueId: `i-1`,
      notificationId: `n-1`,
      userId: `u1`,
    })
    expect(dataByCapability.get(`Bearer cap_2`)).toEqual({
      type: `issue_comment`,
      issueId: `i-1`,
      notificationId: `n-2`,
      userId: `u2`,
    })
  })

  it(`skips the relay entirely when no recipient has a device`, async () => {
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

// EXP-1428: every device is pushed sealed to POST /push (one request per
// device, Bearer capability).
describe(`sendToUsers sealed /push (EXP-1428)`, () => {
  it(`seals the merged payload for the device and skips rows without a capability`, async () => {
    h.selectResults.push([
      row(`u1`, `cap_a`),
      { userId: `u1`, relayCapability: null, pushPublicKey: null },
      { userId: `u2`, relayCapability: `cap_nokey`, pushPublicKey: null },
    ])
    fetchMock.mockResolvedValue(relayOk())

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
    expect(calls).toHaveLength(1)
    expect(calls[0][0]).toBe(`http://relay.test/push`)
    expect(calls.some((c) => (c[0] as string).endsWith(`/send`))).toBe(false)

    const init = calls[0][1] as RequestInit
    expect((init.headers as Record<string, string>).Authorization).toBe(`Bearer cap_a`)
    expect(init.signal).toBeInstanceOf(AbortSignal)
    const { body, payload } = openBody(calls[0])
    expect(body.collapseId).toBe(`issue-i-1`)
    expect(payload).toEqual({
      title: `Hello`,
      body: `World`,
      data: { type: `issue_comment`, issueId: `i-1`, notificationId: `n-1`, userId: `u1` },
    })
    expect(h.deleteCalls.count).toBe(0)
  })

  it(`carries a question's sealedText inside the sealed payload (#VAPP-45)`, async () => {
    h.selectResults.push([row(`u1`, `cap_q`)])
    fetchMock.mockResolvedValue(relayOk())
    const sealedText = `exp-sealed:v1:1:${`A`.repeat(200)}`

    await sendToUsers([{ userId: `u1` }], {
      title: `T needs you`,
      body: `New message in a session`,
      data: { type: `agent_message`, sessionId: `s-1`, rootId: `s-1`, sealedText },
    })

    expect(fetchMock).toHaveBeenCalledTimes(1)
    const { body, payload } = openBody(fetchMock.mock.calls[0])
    expect(JSON.stringify(body)).not.toContain(sealedText)
    expect(payload.data.sealedText).toBe(sealedText)
    expect(payload.data.userId).toBe(`u1`)
    expect(payload.body).toBe(`New message in a session`)
  })

  it(`never posts when every row is bare`, async () => {
    h.selectResults.push([{ userId: `u1`, relayCapability: null, pushPublicKey: null }])

    await sendToUsers([{ userId: `u1` }], { title: `Hi`, data: {} })

    expect(fetchMock).not.toHaveBeenCalled()
    expect(h.deleteCalls.count).toBe(0)
  })

  it(`deletes rows whose install is gone (410) or capability revoked (401)`, async () => {
    h.selectResults.push([row(`u1`, `cap_gone`), row(`u2`, `cap_revoked`), row(`u3`, `cap_busy`)])
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

  // #VAPP-45 (question-push.json SIZE): a question's tree-sealed text is
  // dropped FIRST, before the body is cut or a routing key goes.
  it(`drops a question's sealedText first, keeping the body and every other key`, () => {
    const data = { type: `agent_message`, sessionId: `s`, rootId: `s`, notificationId: `n`, userId: `u` }
    const p = sealedPlaintext(`T needs you`, `New message in a session`, { ...data, sealedText: `exp-sealed:v1:1:${`A`.repeat(4000)}` })
    const json = JSON.parse(Buffer.from(p).toString())
    expect(sealedSize(p)).toBeLessThanOrEqual(MAX_SEALED_BYTES)
    expect(json.data).toEqual(data)
    expect(json.body).toBe(`New message in a session`)
  })

  it(`keeps a sealedText that fits`, () => {
    const data = { type: `agent_message`, rootId: `s`, userId: `u`, sealedText: `exp-sealed:v1:1:${`A`.repeat(1500)}` }
    const json = JSON.parse(Buffer.from(sealedPlaintext(`T`, `B`, data)).toString())
    expect(json.data).toEqual(data)
  })
})
