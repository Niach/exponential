import { createHmac } from "node:crypto"
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { z } from "zod"

import { mintCallId, verifyCallId, type CallIdBinding } from "@/lib/voice-call-id"

const AGENT = `3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d`
const binding: CallIdBinding = { userId: `me`, deviceId: `d-mint`, agentId: AGENT, sealed: false }
const at = (seconds: number) => new Date(Date.UTC(2026, 10, 18, 9, 0, 0) + seconds * 1000)

let before: string | undefined
beforeEach(() => {
  before = process.env.BETTER_AUTH_SECRET
  process.env.BETTER_AUTH_SECRET = `test-secret`
})
afterEach(() => {
  if (before === undefined) delete process.env.BETTER_AUTH_SECRET
  else process.env.BETTER_AUTH_SECRET = before
})

describe(`#EXP-1941 the call id vouches`, () => {
  it(`is a v4-shaped UUID that zod's uuid() takes, different each time`, () => {
    const ids = Array.from({ length: 50 }, () => mintCallId(binding, at(0))!)
    expect(new Set(ids).size).toBe(50)
    for (const id of ids) {
      expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/)
      expect(z.string().uuid().safeParse(id).success).toBe(true)
      expect(verifyCallId(id, binding, at(0))).toBe(true)
    }
  })

  it(`the layout, byte for byte: length-prefixed strings, the mode, the agent's 16 bytes, the minute`, () => {
    const id = mintCallId(binding, at(0))!
    const bytes = Buffer.from(id.replace(/-/g, ``), `hex`)
    const minute = Buffer.alloc(4)
    minute.writeUInt32BE(Math.floor(at(0).getTime() / 60_000))
    const input = Buffer.concat([
      Buffer.from(`exp-call-id:v1:`),
      bytes.subarray(0, 8),
      Buffer.from([0, 2]),
      Buffer.from(`me`),
      Buffer.from([0, 6]),
      Buffer.from(`d-mint`),
      Buffer.from([0]),
      Buffer.from(AGENT.replace(/-/g, ``), `hex`),
      minute,
    ])
    const mac = createHmac(`sha256`, `test-secret`).update(input).digest().subarray(0, 8)
    mac[0] = (mac[0] & 0x3f) | 0x80
    expect(bytes.subarray(8).toString(`hex`)).toBe(mac.toString(`hex`))
  })

  it(`refuses another member, device, agent or mode, and an id it did not mint`, () => {
    const id = mintCallId(binding, at(0))!
    expect(verifyCallId(id, { ...binding, userId: `mate` }, at(0))).toBe(false)
    expect(verifyCallId(id, { ...binding, deviceId: `d-mac` }, at(0))).toBe(false)
    expect(verifyCallId(id, { ...binding, agentId: `4f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d` }, at(0))).toBe(false)
    expect(verifyCallId(id, { ...binding, sealed: true }, at(0))).toBe(false)
    expect(verifyCallId(crypto.randomUUID(), binding, at(0))).toBe(false)
    expect(verifyCallId(`not-a-uuid`, binding, at(0))).toBe(false)
    // One flipped bit of the random half or of the MAC half.
    const flip = (index: number) => id.slice(0, index) + (id[index] === `0` ? `1` : `0`) + id.slice(index + 1)
    expect(verifyCallId(flip(0), binding, at(0))).toBe(false)
    expect(verifyCallId(flip(35), binding, at(0))).toBe(false)
    // The uppercase form of the same id is the same id.
    expect(verifyCallId(id.toUpperCase(), binding, at(0))).toBe(true)
  })

  it(`no two tuples share an input: a boundary moved between the member and the device does not verify`, () => {
    const id = mintCallId({ ...binding, userId: `ab`, deviceId: `c` }, at(0))!
    expect(verifyCallId(id, { ...binding, userId: `a`, deviceId: `bc` }, at(0))).toBe(false)
    expect(verifyCallId(id, { ...binding, userId: `ab`, deviceId: `c` }, at(0))).toBe(true)
  })

  it(`lives two to three minutes, and one minute ahead of the verifying clock`, () => {
    const id = mintCallId(binding, at(59))!
    expect(verifyCallId(id, binding, at(59))).toBe(true)
    // Minted in minute 0: good through minute 2, gone in minute 3.
    expect(verifyCallId(id, binding, at(60 * 2 + 59))).toBe(true)
    expect(verifyCallId(id, binding, at(60 * 3))).toBe(false)
    // A verifier whose clock is up to a minute behind the minting process.
    expect(verifyCallId(id, binding, at(-1))).toBe(true)
    expect(verifyCallId(id, binding, at(-61))).toBe(false)
  })

  it(`a sealed id verifies only as sealed`, () => {
    const id = mintCallId({ ...binding, sealed: true }, at(0))!
    expect(verifyCallId(id, { ...binding, sealed: true }, at(0))).toBe(true)
    expect(verifyCallId(id, binding, at(0))).toBe(false)
  })

  it(`without the secret nothing is minted and nothing verifies; another secret verifies nothing`, () => {
    const id = mintCallId(binding, at(0))!
    process.env.BETTER_AUTH_SECRET = `rotated`
    expect(verifyCallId(id, binding, at(0))).toBe(false)
    delete process.env.BETTER_AUTH_SECRET
    expect(mintCallId(binding, at(0))).toBeNull()
    expect(verifyCallId(id, binding, at(0))).toBe(false)
  })

  it(`an agent id that is no UUID mints nothing`, () => {
    expect(mintCallId({ ...binding, agentId: `agent-1` }, at(0))).toBeNull()
  })
})
