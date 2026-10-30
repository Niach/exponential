// @vitest-environment node
import { createPrivateKey, createPublicKey, sign } from "node:crypto"
import { describe, expect, it, vi } from "vitest"
import fixture from "../../../../../packages/domain-contract/fixtures/install-register.json"

// EXP-1457: the browser install's `installs.register` input. The server's
// own router accepts it, the message is byte-equal to the server's, and it
// replays the shared vector fixtures/install-register.json.

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth/access`, () => ({ resolveTeamAccess: vi.fn(async () => ({ kind: `member` })) }))

import { installMessage as serverInstallMessage, installsRouter } from "@/lib/trpc/installs"
import { createFakeDb } from "@/lib/mcp-oauth/test-db"
import {
  browserLabel,
  buildRegistration,
  installMessage,
  type InstallCrypto,
} from "./install-registration"

const PKCS8_ED25519 = Buffer.from(`302e020100300506032b657004220420`, `hex`)
const hex = (s: string) => new Uint8Array(Buffer.from(s, `hex`))

// What exponential-net-wasm's `identitySign` / `pushPublicKey` compute, in
// node:crypto: ed25519 over the raw 32 secret bytes; the push key of the
// fixture identity is the fixture's (the real wasm derivation is replayed
// against the same vector by scripts/enet-wasm-smoke.ts).
const nodeCrypto: InstallCrypto = {
  identitySign: (identity, message) =>
    new Uint8Array(
      sign(null, message, createPrivateKey({ key: Buffer.concat([PKCS8_ED25519, identity]), format: `der`, type: `pkcs8` }))
    ),
  pushPublicKey: (identity) => {
    expect(Buffer.from(identity).toString(`hex`)).toBe(fixture.identity)
    return new Uint8Array(Buffer.from(fixture.x25519Public, `base64url`))
  },
}

function endpointIdOf(identity: Uint8Array): string {
  const key = createPrivateKey({ key: Buffer.concat([PKCS8_ED25519, identity]), format: `der`, type: `pkcs8` })
  return createPublicKey(key).export({ format: `der`, type: `spki` }).subarray(12).toString(`hex`)
}

const IDENTITY = hex(fixture.identity)
const USER = fixture.userId

describe(`installs.register input (web)`, () => {
  it(`signs the contract string: byte-equal to the server's`, () => {
    const args = [USER, `ab`.repeat(32), `x`, `2026-10-30T12:00:00.000Z`] as const
    expect(installMessage(...args)).toBe(serverInstallMessage(...args))
    expect(installMessage(...args)).toBe(
      `exp-install-v1\nfixture-user-1\n${`ab`.repeat(32)}\nx\n2026-10-30T12:00:00.000Z`
    )
  })

  it(`replays fixtures/install-register.json`, () => {
    expect(endpointIdOf(IDENTITY)).toBe(fixture.endpointId)
    expect(installMessage(fixture.userId, fixture.endpointId, fixture.x25519Public, fixture.signedAt)).toBe(
      fixture.message
    )
    const reg = buildRegistration({
      userId: USER,
      identity: IDENTITY,
      endpointId: fixture.endpointId.toUpperCase(),
      crypto: nodeCrypto,
      now: new Date(fixture.signedAt),
      userAgent: `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/141.0 Safari/537.36`,
    })
    expect(reg).toEqual({
      kind: `web`,
      label: `Chrome on macOS`,
      platform: `macOS`,
      endpointId: fixture.endpointId,
      x25519Public: fixture.x25519Public,
      signedAt: fixture.signedAt,
      signature: fixture.signature,
    })
    expect(Buffer.from(reg.signature, `base64url`).toString(`hex`)).toBe(fixture.signatureHex)
  })

  it(`is accepted by the server's installs.register`, async () => {
    const reg = buildRegistration({
      userId: USER,
      identity: IDENTITY,
      endpointId: endpointIdOf(IDENTITY),
      crypto: nodeCrypto,
    })
    const db = createFakeDb({ installs: [], devices: [], team_members: [] })
    const caller = installsRouter.createCaller({
      session: { user: { id: USER, name: USER, email: `u@example.com` }, session: { id: `s` } },
      db,
      request: new Request(`http://localhost/`),
    } as never)
    const { installId } = await caller.register(reg)
    expect(installId).toBeTruthy()
    // Signed for another account = refused.
    const other = installsRouter.createCaller({
      session: { user: { id: `someone_else`, name: `x`, email: `x@example.com` }, session: { id: `s2` } },
      db,
      request: new Request(`http://localhost/`),
    } as never)
    await expect(other.register(reg)).rejects.toThrow(/signature/)
  })

  it(`refuses a malformed endpoint id or signature`, () => {
    expect(() =>
      buildRegistration({ userId: USER, identity: IDENTITY, endpointId: `nope`, crypto: nodeCrypto })
    ).toThrow(/hex/)
    expect(() =>
      buildRegistration({
        userId: USER,
        identity: IDENTITY,
        endpointId: endpointIdOf(IDENTITY),
        crypto: { ...nodeCrypto, identitySign: () => new Uint8Array(10) },
      })
    ).toThrow(/64 bytes/)
  })

  it(`labels the browser`, () => {
    expect(browserLabel(`Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0`)).toEqual({
      label: `Firefox on Linux`,
      platform: `Linux`,
    })
    expect(browserLabel(``)).toEqual({ label: `Browser`, platform: `web` })
  })
})
