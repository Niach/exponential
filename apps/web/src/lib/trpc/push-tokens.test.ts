import { beforeEach, describe, expect, it, vi } from "vitest"

// pushTokens.register (EXP-1428): released clients keep sending
// {token, platform}; new ones add the push relay capability + their X25519
// push public key (both or neither), which the row stores so sendToUsers can
// seal to it. A re-register without them falls back to the /send shim.

vi.mock(`@/db/connection`, () => ({ db: {} }))

import { pushTokensRouter } from "@/lib/trpc/push-tokens"
import { createFakeDb, type FakeDb } from "@/lib/mcp-oauth/test-db"

const PUB = Buffer.alloc(32, 7).toString(`base64url`)
let db: FakeDb

function caller() {
  return pushTokensRouter.createCaller({
    session: {
      user: { id: `u1`, name: `U`, email: `u@example.com` },
      session: { id: `sess-1` },
    },
    db,
    request: new Request(`http://localhost/`),
  } as never)
}

beforeEach(() => {
  db = createFakeDb()
})

describe(`pushTokens.register`, () => {
  it(`keeps the released {token, platform} shape working`, async () => {
    await caller().register({ token: `t1`, platform: `ios` })
    expect(db.rows(`fcm_tokens`)).toMatchObject([
      { userId: `u1`, token: `t1`, platform: `ios`, relayCapability: null, pushPublicKey: null },
    ])
  })

  it(`stores a capability + push public key, and clears them on a plain re-register`, async () => {
    await caller().register({ token: `t1`, platform: `android`, capability: `cap_abc`, pushPublicKey: PUB })
    expect(db.rows(`fcm_tokens`)).toMatchObject([
      { token: `t1`, relayCapability: `cap_abc`, pushPublicKey: PUB },
    ])
    await caller().register({ token: `t1`, platform: `android` })
    expect(db.rows(`fcm_tokens`)).toHaveLength(1)
    expect(db.rows(`fcm_tokens`)[0]).toMatchObject({ relayCapability: null, pushPublicKey: null })
  })

  it(`refuses one without the other, a non-cap_ capability and a bad key`, async () => {
    await expect(caller().register({ token: `t`, platform: `ios`, capability: `cap_x` })).rejects.toThrow()
    await expect(caller().register({ token: `t`, platform: `ios`, pushPublicKey: PUB })).rejects.toThrow()
    await expect(
      caller().register({ token: `t`, platform: `ios`, capability: `key_x`, pushPublicKey: PUB })
    ).rejects.toThrow()
    for (const bad of [
      Buffer.alloc(31, 7).toString(`base64url`),
      Buffer.alloc(33, 7).toString(`base64url`),
      Buffer.alloc(32, 7).toString(`base64`).replace(/=+$/, `+`),
      `${PUB}=`,
    ]) {
      await expect(
        caller().register({ token: `t`, platform: `ios`, capability: `cap_x`, pushPublicKey: bad })
      ).rejects.toThrow()
    }
    expect(db.rows(`fcm_tokens`)).toHaveLength(0)
  })
})

describe(`pushTokens.relay`, () => {
  it(`hands the devices the public relay URL, or null without a relay`, async () => {
    vi.stubEnv(`PUSH_RELAY_URL`, ``)
    vi.stubEnv(`PUSH_RELAY_PUBLIC_URL`, ``)
    expect(await caller().relay()).toEqual({ url: null })
    vi.stubEnv(`PUSH_RELAY_URL`, `http://push-relay:4001`)
    expect(await caller().relay()).toEqual({ url: `http://push-relay:4001` })
    vi.stubEnv(`PUSH_RELAY_PUBLIC_URL`, `https://push.exponential.at/`)
    expect(await caller().relay()).toEqual({ url: `https://push.exponential.at` })
    vi.unstubAllEnvs()
  })
})
