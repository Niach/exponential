import { beforeEach, describe, expect, it, vi } from "vitest"
import { generateKeyPairSync, randomBytes, sign, type KeyObject } from "node:crypto"

// installs (EXP-1457 / VAPP-37): the eNet install registry. Registration
// needs a fresh ed25519 signature by the install identity over the caller's
// user id; an endpoint belongs to one account; revoked is final; `list` is
// the non-revoked installs of the team's current members, never another
// team's.

vi.mock(`@/db/connection`, () => ({ db: {} }))

let db: FakeDb
vi.mock(`@/lib/auth/access`, () => ({
  resolveTeamAccess: vi.fn(async (userId: string, teamId: string) => {
    const member = db
      .rows(`team_members`)
      .some((row) => row.userId === userId && row.teamId === teamId)
    if (!member) {
      const { TRPCError } = await import(`@trpc/server`)
      throw new TRPCError({ code: `FORBIDDEN`, message: `Not a team member` })
    }
    return { kind: `member` }
  }),
}))

// VAPP-33: forSession reads the session through the visibility helpers.
const SESSIONS: Record<string, { id: string; userId: string; shared: boolean; teamId: string; deviceId: string }> = {}
vi.mock(`@/lib/sessions/access`, () => ({
  loadSessionForUser: vi.fn(async (userId: string, sessionId: string) => {
    const session = SESSIONS[sessionId]
    if (!session || !(session.shared || session.userId === userId)) {
      const { TRPCError } = await import(`@trpc/server`)
      throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
    }
    return session
  }),
  sessionDeviceOwner: vi.fn(async (session: { userId: string }) => session.userId),
}))

import { installMessage, installsRouter, MAX_INSTALLS_PER_USER } from "@/lib/trpc/installs"
import { createFakeDb, type FakeDb } from "@/lib/mcp-oauth/test-db"
import registerFixture from "@exp/domain-contract/fixtures/install-register.json"

const TEAM_A = `00000000-0000-4000-8000-00000000000a`
const TEAM_B = `00000000-0000-4000-8000-00000000000b`

function caller(userId: string) {
  return installsRouter.createCaller({
    session: {
      user: { id: userId, name: userId, email: `${userId}@example.com` },
      session: { id: `sess-${userId}` },
    },
    db,
    request: new Request(`http://localhost/`),
  } as never)
}

type Identity = { endpointId: string; privateKey: KeyObject }

function identity(): Identity {
  const { publicKey, privateKey } = generateKeyPairSync(`ed25519`)
  const raw = publicKey.export({ format: `der`, type: `spki` }).subarray(12)
  return { endpointId: raw.toString(`hex`), privateKey }
}

const x25519 = () => randomBytes(32).toString(`base64url`)

function registration(
  userId: string,
  id: Identity,
  opts: { signedAt?: string; signFor?: string; x?: string; kind?: `desktop` | `daemon` | `ios` | `android` | `web`; deviceId?: string; label?: string } = {}
) {
  const x25519Public = opts.x ?? x25519()
  const signedAt = opts.signedAt ?? new Date().toISOString()
  const message = installMessage(opts.signFor ?? userId, id.endpointId, x25519Public, signedAt)
  return {
    kind: opts.kind ?? `ios`,
    label: opts.label,
    platform: `iOS 27`,
    deviceId: opts.deviceId,
    endpointId: id.endpointId,
    x25519Public,
    signedAt,
    signature: sign(null, Buffer.from(message, `utf8`), id.privateKey).toString(`base64url`),
  }
}

beforeEach(() => {
  db = createFakeDb({
    team_members: [
      { teamId: TEAM_A, userId: `u1` },
      { teamId: TEAM_A, userId: `u2` },
      { teamId: TEAM_B, userId: `u3` },
    ],
    devices: [
      { id: `11111111-1111-4111-8111-111111111111`, userId: `u1`, deviceId: `mac-1` },
      { id: `22222222-2222-4222-8222-222222222222`, userId: `u2`, deviceId: `mac-2` },
    ],
  })
})

describe(`installs.register`, () => {
  it(`accepts the shared fixture vector every install signs byte-exact`, async () => {
    const fx = registerFixture
    expect(installMessage(fx.userId, fx.endpointId, fx.x25519Public, fx.signedAt)).toBe(fx.message)
    expect(Buffer.from(fx.signature, `base64url`).toString(`hex`)).toBe(fx.signatureHex)
    vi.useFakeTimers({ toFake: [`Date`] })
    vi.setSystemTime(new Date(fx.signedAt))
    try {
      const { installId } = await caller(fx.userId).register({
        kind: `android`,
        endpointId: fx.endpointId,
        x25519Public: fx.x25519Public,
        signedAt: fx.signedAt,
        signature: fx.signature,
      })
      expect(installId).toBeTruthy()
    } finally {
      vi.useRealTimers()
    }
  })

  it(`stores a correctly signed install`, async () => {
    const id = identity()
    const input = registration(`u1`, id, { label: `Danny's phone` })
    const { installId } = await caller(`u1`).register(input)
    expect(db.rows(`installs`)).toMatchObject([
      {
        id: installId,
        userId: `u1`,
        kind: `ios`,
        label: `Danny's phone`,
        platform: `iOS 27`,
        deviceId: null,
        endpointId: id.endpointId,
        x25519Public: input.x25519Public,
      },
    ])
  })

  it(`refuses a bad signature, one for another user, and a malformed one`, async () => {
    const id = identity()
    const other = identity()
    const forged = { ...registration(`u1`, other), endpointId: id.endpointId }
    await expect(caller(`u1`).register(forged)).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(caller(`u1`).register(registration(`u1`, id, { signFor: `u2` }))).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    const tampered = registration(`u1`, id)
    await expect(caller(`u1`).register({ ...tampered, x25519Public: x25519() })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    await expect(caller(`u1`).register({ ...tampered, signature: `abc` })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    await expect(
      caller(`u1`).register({ ...tampered, endpointId: id.endpointId.toUpperCase() })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(db.rows(`installs`)).toHaveLength(0)
  })

  it(`refuses a signedAt outside ±10 minutes or not a timestamp`, async () => {
    const id = identity()
    const old = new Date(Date.now() - 11 * 60 * 1000).toISOString()
    const ahead = new Date(Date.now() + 11 * 60 * 1000).toISOString()
    for (const signedAt of [old, ahead, `yesterday`]) {
      await expect(caller(`u1`).register(registration(`u1`, id, { signedAt }))).rejects.toMatchObject({
        code: `BAD_REQUEST`,
      })
    }
    const recent = new Date(Date.now() - 9 * 60 * 1000).toISOString()
    await caller(`u1`).register(registration(`u1`, id, { signedAt: recent }))
    expect(db.rows(`installs`)).toHaveLength(1)
  })

  it(`is an idempotent upsert for the same user + endpoint`, async () => {
    const id = identity()
    const first = await caller(`u1`).register(registration(`u1`, id, { label: `old` }))
    const seen = db.rows(`installs`)[0]!.lastSeenAt
    const x = x25519()
    const again = await caller(`u1`).register(
      registration(`u1`, id, { label: `new`, x, kind: `desktop`, deviceId: `mac-1` })
    )
    expect(again.installId).toBe(first.installId)
    expect(db.rows(`installs`)).toHaveLength(1)
    expect(db.rows(`installs`)[0]).toMatchObject({
      label: `new`,
      x25519Public: x,
      kind: `desktop`,
      deviceId: `11111111-1111-4111-8111-111111111111`,
    })
    expect(db.rows(`installs`)[0]!.lastSeenAt).not.toBe(seen)
  })

  it(`resolves deviceId to the caller's own devices row only, desktop/daemon only`, async () => {
    await expect(
      caller(`u1`).register(registration(`u1`, identity(), { kind: `daemon`, deviceId: `mac-2` }))
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      caller(`u1`).register(registration(`u1`, identity(), { kind: `ios`, deviceId: `mac-1` }))
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await caller(`u1`).register(registration(`u1`, identity(), { kind: `daemon`, deviceId: `mac-1` }))
    expect(db.rows(`installs`)).toMatchObject([
      { kind: `daemon`, deviceId: `11111111-1111-4111-8111-111111111111` },
    ])
  })

  it(`refuses an endpoint another account registered (CONFLICT)`, async () => {
    const id = identity()
    await caller(`u1`).register(registration(`u1`, id))
    await expect(caller(`u2`).register(registration(`u2`, id))).rejects.toMatchObject({ code: `CONFLICT` })
    expect(db.rows(`installs`)).toMatchObject([{ userId: `u1` }])
  })

  it(`refuses a revoked endpoint (FORBIDDEN)`, async () => {
    const id = identity()
    const { installId } = await caller(`u1`).register(registration(`u1`, id))
    await caller(`u1`).revoke({ installId })
    await expect(caller(`u1`).register(registration(`u1`, id))).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`caps live installs per user; revoking frees a slot`, async () => {
    const ids: string[] = []
    for (let i = 0; i < MAX_INSTALLS_PER_USER; i++) {
      ids.push((await caller(`u1`).register(registration(`u1`, identity()))).installId)
    }
    await expect(caller(`u1`).register(registration(`u1`, identity()))).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    // Re-registering an existing install is no new slot.
    expect(db.rows(`installs`)).toHaveLength(MAX_INSTALLS_PER_USER)
    await caller(`u1`).revoke({ installId: ids[0]! })
    await caller(`u1`).register(registration(`u1`, identity()))
    expect(db.rows(`installs`)).toHaveLength(MAX_INSTALLS_PER_USER + 1)
    // Another user is unaffected.
    await caller(`u2`).register(registration(`u2`, identity()))
  })
})

describe(`installs.list`, () => {
  it(`returns the team members' live installs, never another team's`, async () => {
    const a1 = identity()
    const a2 = identity()
    const revoked = identity()
    const b3 = identity()
    await caller(`u1`).register(registration(`u1`, a1))
    await caller(`u2`).register(registration(`u2`, a2, { kind: `android` }))
    const { installId } = await caller(`u2`).register(registration(`u2`, revoked))
    await caller(`u2`).revoke({ installId })
    await caller(`u3`).register(registration(`u3`, b3))

    const listed = await caller(`u1`).list({ teamId: TEAM_A })
    expect(listed.map((row) => row.endpointId).sort()).toEqual([a1.endpointId, a2.endpointId].sort())
    expect(listed.find((row) => row.endpointId === a2.endpointId)).toMatchObject({
      userId: `u2`,
      kind: `android`,
      platform: `iOS 27`,
    })
    expect(Object.keys(listed[0]!).sort()).toEqual(
      [`installId`, `userId`, `deviceId`, `kind`, `label`, `platform`, `endpointId`, `x25519Public`, `lastSeenAt`].sort()
    )

    expect((await caller(`u3`).list({ teamId: TEAM_B })).map((row) => row.endpointId)).toEqual([b3.endpointId])
  })

  it(`refuses a non-member`, async () => {
    await expect(caller(`u3`).list({ teamId: TEAM_A })).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(caller(`u1`).list({ teamId: TEAM_B })).rejects.toMatchObject({ code: `FORBIDDEN` })
  })
})

describe(`installs.forSession (VAPP-33 discovery)`, () => {
  const S1 = `33333333-3333-4333-8333-333333333333`
  const S2 = `44444444-4444-4444-8444-444444444444`
  beforeEach(() => {
    SESSIONS[S1] = { id: S1, userId: `u1`, shared: false, teamId: TEAM_A, deviceId: `mac-1` }
    SESSIONS[S2] = { id: S2, userId: `u1`, shared: true, teamId: TEAM_A, deviceId: `mac-1` }
  })

  it(`returns the live desktop/daemon installs of the running machine, never phones or other machines`, async () => {
    const desktop = identity()
    const daemon = identity()
    const phone = identity()
    const gone = identity()
    const other = identity()
    await caller(`u1`).register(registration(`u1`, desktop, { kind: `desktop`, deviceId: `mac-1` }))
    await caller(`u1`).register(registration(`u1`, daemon, { kind: `daemon`, deviceId: `mac-1` }))
    await caller(`u1`).register(registration(`u1`, phone, { kind: `ios` }))
    const { installId } = await caller(`u1`).register(registration(`u1`, gone, { kind: `desktop`, deviceId: `mac-1` }))
    await caller(`u1`).revoke({ installId })
    await caller(`u2`).register(registration(`u2`, other, { kind: `desktop`, deviceId: `mac-2` }))

    const found = await caller(`u1`).forSession({ sessionId: S1 })
    expect(found.map((row) => row.endpointId).sort()).toEqual([desktop.endpointId, daemon.endpointId].sort())
    expect(Object.keys(found[0]!).sort()).toEqual([`endpointId`, `kind`])
  })

  it(`follows the visibility rule: a teammate sees a shared session's installs, not a private one's`, async () => {
    await caller(`u1`).register(registration(`u1`, identity(), { kind: `desktop`, deviceId: `mac-1` }))
    expect(await caller(`u2`).forSession({ sessionId: S2 })).toHaveLength(1)
    await expect(caller(`u2`).forSession({ sessionId: S1 })).rejects.toMatchObject({ code: `NOT_FOUND` })
  })
})

describe(`installs.mine + revoke`, () => {
  it(`lists the caller's own installs including revoked ones`, async () => {
    const keep = await caller(`u1`).register(registration(`u1`, identity()))
    const gone = await caller(`u1`).register(registration(`u1`, identity()))
    await caller(`u2`).register(registration(`u2`, identity()))
    await caller(`u1`).revoke({ installId: gone.installId })

    const mine = await caller(`u1`).mine()
    expect(mine).toHaveLength(2)
    expect(mine.find((row) => row.installId === keep.installId)!.revokedAt).toBeFalsy()
    expect(mine.find((row) => row.installId === gone.installId)!.revokedAt).toBeInstanceOf(Date)
  })

  it(`revokes only the caller's own installs (NOT_FOUND otherwise)`, async () => {
    const { installId } = await caller(`u1`).register(registration(`u1`, identity()))
    await expect(caller(`u2`).revoke({ installId })).rejects.toMatchObject({ code: `NOT_FOUND` })
    await expect(
      caller(`u1`).revoke({ installId: `99999999-9999-4999-8999-999999999999` })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    expect(db.rows(`installs`)[0]!.revokedAt).toBeFalsy()
    await caller(`u1`).revoke({ installId })
    expect(db.rows(`installs`)[0]!.revokedAt).toBeInstanceOf(Date)
  })
})
