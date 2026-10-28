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
// (the pin, the machine's rows eldest first, the squats); writes are recorded.
// The SQL itself (ordering, cardinality) is covered by the real-db suite.
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

const OLD = new Date(`2026-01-01T00:00:00Z`)
const MID = new Date(`2026-05-01T00:00:00Z`)
const NEW = new Date(`2026-09-01T00:00:00Z`)
const KEY = `ab`.repeat(32)
const pin = (args: { userId: string; deviceId: string; publicKey: string }) => pinDeviceKeyInTx(tx as never, args)

describe(`pinDeviceKeyInTx (FEED-159: the eldest row pins)`, () => {
  beforeEach(() => {
    t.selects = []
    t.inserted = []
    t.updated = []
  })

  it(`pins first for the account of the machine's eldest row`, async () => {
    t.selects = [[], [{ userId: `owner`, createdAt: OLD, sharedTeamIds: [] }, { userId: `squat`, createdAt: NEW, sharedTeamIds: [] }], []]
    expect(await pin({ userId: `owner`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: true, squats: [] })
    expect(t.inserted).toEqual([{ deviceId: `dev-1`, publicKey: KEY, userId: `owner` }])
  })

  it(`refuses a first pin for a younger account that learned the id`, async () => {
    t.selects = [[], [{ userId: `owner`, createdAt: OLD, sharedTeamIds: [] }, { userId: `squat`, createdAt: NEW, sharedTeamIds: [] }]]
    expect(await pin({ userId: `squat`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: false, squats: [] })
    expect(t.inserted).toEqual([])
  })

  it(`keeps the bootstrap rule: no first pin while only another account's row shares`, async () => {
    t.selects = [[], [{ userId: `eldest`, createdAt: OLD, sharedTeamIds: [] }, { userId: `host`, createdAt: NEW, sharedTeamIds: [`t`] }]]
    expect(await pin({ userId: `eldest`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: false, squats: [] })
    expect(t.inserted).toEqual([])
  })

  it(`the eldest row's account re-pins over a younger account's pin and withdraws its shares`, async () => {
    const squatRow = { id: `row-s`, userId: `squat`, deviceId: `dev-1`, label: `box`, sharedTeamIds: [`t`] }
    t.selects = [
      [{ publicKey: `ef`.repeat(32), userId: `squat`, createdAt: MID }],
      [{ userId: `owner`, createdAt: OLD, sharedTeamIds: [] }, { userId: `squat`, createdAt: new Date(`2026-02-01T00:00:00Z`), sharedTeamIds: [`t`] }],
      [squatRow],
    ]
    expect(await pin({ userId: `owner`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: true, squats: [squatRow] })
    expect(t.updated).toEqual([expect.objectContaining({ publicKey: KEY, userId: `owner` })])
  })

  it(`never takes over from a holder whose row was re-created after it pinned (Remove + launch)`, async () => {
    t.selects = [
      [{ publicKey: `ef`.repeat(32), userId: `owner`, createdAt: MID }],
      [{ userId: `squat`, createdAt: OLD, sharedTeamIds: [] }, { userId: `owner`, createdAt: NEW, sharedTeamIds: [] }],
    ]
    expect(await pin({ userId: `squat`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: false, squats: [] })
    expect(t.updated).toEqual([])
  })

  it(`never takes over from a holder whose row is gone`, async () => {
    t.selects = [[{ publicKey: `ef`.repeat(32), userId: `owner`, createdAt: MID }], [{ userId: `squat`, createdAt: OLD, sharedTeamIds: [] }]]
    expect(await pin({ userId: `squat`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: false, squats: [] })
  })

  it(`a younger account never takes over the eldest's pin`, async () => {
    t.selects = [
      [{ publicKey: `ef`.repeat(32), userId: `owner`, createdAt: MID }],
      [{ userId: `owner`, createdAt: OLD, sharedTeamIds: [] }, { userId: `squat`, createdAt: NEW, sharedTeamIds: [] }],
    ]
    expect(await pin({ userId: `squat`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: false, squats: [] })
  })

  it(`the holder rotates its own key whatever its row's age`, async () => {
    t.selects = [[{ publicKey: `ef`.repeat(32), userId: `owner`, createdAt: MID }], []]
    expect(await pin({ userId: `owner`, deviceId: `dev-1`, publicKey: KEY })).toEqual({ pinned: true, squats: [] })
    expect(t.updated).toEqual([expect.objectContaining({ publicKey: KEY })])
    expect(t.updated[0]).not.toHaveProperty(`userId`)
  })
})
