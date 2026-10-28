import { generateKeyPairSync, sign } from "node:crypto"
import { beforeEach, describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import {
  checkDeviceKeyProof,
  deviceKeyMessage,
  deviceKeyRejectionLog,
  DEVICE_KEY_PROOF_SKEW_MS,
  pinDeviceKeyInTx,
} from "@/lib/device-keys"

// A real ed25519 machine key, signing like the CLI daemon does.
function machineKey() {
  const { publicKey, privateKey } = generateKeyPairSync(`ed25519`)
  const hex = Buffer.from(publicKey.export({ format: `jwk` }).x!, `base64url`).toString(`hex`)
  const prove = (userId: string, deviceId: string, signedAt: string) => ({
    publicKey: hex,
    signedAt,
    signature: sign(null, Buffer.from(deviceKeyMessage(userId, deviceId, hex, signedAt)), privateKey).toString(`hex`),
  })
  return { hex, prove }
}

describe(`checkDeviceKeyProof (FEED-159: clock skew is diagnosable)`, () => {
  const now = new Date(`2026-10-28T12:00:00.000Z`)
  const key = machineKey()

  it(`accepts a fresh proof`, () => {
    const proof = key.prove(`u`, `dev-1`, now.toISOString())
    expect(checkDeviceKeyProof({ userId: `u`, deviceId: `dev-1`, proof }, now)).toEqual({ ok: true })
  })

  it(`rejects a stale or future signedAt with the signed skew`, () => {
    const behind = key.prove(`u`, `dev-1`, new Date(now.getTime() - DEVICE_KEY_PROOF_SKEW_MS - 5_000).toISOString())
    expect(checkDeviceKeyProof({ userId: `u`, deviceId: `dev-1`, proof: behind }, now)).toEqual({
      ok: false,
      reason: `skew`,
      skewMs: -(DEVICE_KEY_PROOF_SKEW_MS + 5_000),
    })
    const ahead = key.prove(`u`, `dev-1`, new Date(now.getTime() + 3_600_000).toISOString())
    expect(checkDeviceKeyProof({ userId: `u`, deviceId: `dev-1`, proof: ahead }, now)).toEqual({
      ok: false,
      reason: `skew`,
      skewMs: 3_600_000,
    })
  })

  it(`rejects another account's proof and a malformed one`, () => {
    const proof = key.prove(`u`, `dev-1`, now.toISOString())
    expect(checkDeviceKeyProof({ userId: `thief`, deviceId: `dev-1`, proof }, now)).toEqual({ ok: false, reason: `signature` })
    expect(checkDeviceKeyProof({ userId: `u`, deviceId: `dev-1`, proof: { ...proof, signedAt: `soon` } }, now)).toEqual({
      ok: false,
      reason: `format`,
    })
  })

  it(`logs the machine and how far its clock is off`, () => {
    expect(deviceKeyRejectionLog(`dev-1`, { ok: false, reason: `skew`, skewMs: 3_600_000 })).toContain(
      `dev-1: its clock is 3600000 ms ahead of the server's`
    )
    expect(deviceKeyRejectionLog(`dev-1`, { ok: false, reason: `skew`, skewMs: -700_000 })).toContain(
      `dev-1: its clock is 700000 ms behind the server's`
    )
  })
})

// A transaction fake for pinDeviceKeyInTx: its selects resolve in call order
// (the pin, the first-seen account, the sharing rows, the squats); writes are
// recorded. The SQL itself (Remove + relaunch, the admin's pinTo) is covered
// by host-spoofing.real-db.test.ts.
const t = vi.hoisted(() => ({
  selects: [] as unknown[][],
  inserted: [] as unknown[],
  updated: [] as unknown[],
}))
const tx = {
  select: () => {
    const rows = () => Promise.resolve(t.selects.shift() ?? [])
    const chain = {
      from: () => chain,
      where: () => ({
        limit: rows,
        orderBy: rows,
        then: (ok: (value: unknown) => unknown, fail?: (reason: unknown) => unknown) => rows().then(ok, fail),
      }),
    }
    return chain
  },
  insert: () => ({
    values: (values: unknown) => ({
      onConflictDoNothing: async () => {
        t.inserted.push(values)
      },
    }),
  }),
  update: () => ({
    set: (set: unknown) => ({
      where: async () => {
        t.updated.push(set)
      },
    }),
  }),
}

const KEY = `ab`.repeat(32)
const OTHER = `ef`.repeat(32)
const pin = (args: { userId: string; deviceId: string; publicKey: string }) => pinDeviceKeyInTx(tx as never, args)

describe(`pinDeviceKeyInTx (FEED-159: the first-seen account pins)`, () => {
  beforeEach(() => {
    t.selects = []
    t.inserted = []
    t.updated = []
  })

  it(`pins first for the machine's first-seen account`, async () => {
    t.selects = [[], [{ userId: `owner` }], [], []]
    expect(await pin({ userId: `owner`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: true, squats: [] })
    expect(t.inserted).toEqual([{ deviceId: `dev-1`, publicKey: KEY, userId: `owner` }])
  })

  it(`refuses a first pin for any other account`, async () => {
    t.selects = [[], [{ userId: `owner` }]]
    expect(await pin({ userId: `squat`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: false, squats: [] })
    expect(t.inserted).toEqual([])
  })

  it(`keeps the bootstrap rule: no first pin while only another account's row shares`, async () => {
    t.selects = [[], [{ userId: `owner` }], [{ userId: `host` }]]
    expect(await pin({ userId: `owner`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: false, squats: [] })
    expect(t.inserted).toEqual([])
  })

  it(`the first-seen account re-pins over another account's pin and withdraws its shares`, async () => {
    const squatRow = { id: `row-s`, userId: `squat`, deviceId: `dev-1`, label: `box`, sharedTeamIds: [`t`] }
    t.selects = [[{ publicKey: OTHER, userId: `squat` }], [{ userId: `owner` }], [squatRow]]
    expect(await pin({ userId: `owner`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: true, squats: [squatRow] })
    expect(t.updated).toEqual([expect.objectContaining({ publicKey: KEY, userId: `owner` })])
  })

  it(`any other account never takes over the pin`, async () => {
    t.selects = [[{ publicKey: OTHER, userId: `owner` }], [{ userId: `owner` }]]
    expect(await pin({ userId: `squat`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: false, squats: [] })
    expect(t.updated).toEqual([])
  })

  it(`a second account proving the pinned key is accepted and leaves the pin as it is`, async () => {
    t.selects = [[{ publicKey: KEY, userId: `owner` }], []]
    expect(await pin({ userId: `second`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: true, squats: [] })
    expect(t.updated).toEqual([])
    expect(t.inserted).toEqual([])
  })

  it(`the holder rotates its own key`, async () => {
    t.selects = [[{ publicKey: OTHER, userId: `owner` }], []]
    expect(await pin({ userId: `owner`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: true, squats: [] })
    expect(t.updated).toEqual([expect.objectContaining({ publicKey: KEY })])
    expect(t.updated[0]).not.toHaveProperty(`userId`)
  })
})
