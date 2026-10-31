import { beforeEach, describe, expect, it, vi } from "vitest"

// sessionKeys (EXP-1458, fixtures/sealed-content.json): the key API of
// sealed session content. A tree's first key is epoch 1, rotations are the
// starter's or the executing device's; the caller always wraps for itself;
// `pending`/`wrap` only cover readers missing the current epoch on trees the
// caller holds; readers = the starter's installs, every member's when
// shared, and the executing device's; revoked installs and other teams never.

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/trpc`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/trpc")>()),
  generateTxId: vi.fn(async () => 42),
}))

import { sessionKeysRouter, treeReaders } from "@/lib/trpc/session-keys"
import { createFakeDb, type FakeDb } from "@/lib/emcp/upstream/test-db"
import { generateTreeKey, unwrapTreeKey, wrapTreeKey } from "@exp/domain-contract/sealed-content"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"

let db: FakeDb

const TEAM_A = `00000000-0000-4000-8000-00000000000a`
const TEAM_B = `00000000-0000-4000-8000-00000000000b`
const id = (n: string) => `00000000-0000-4000-8000-${n.padStart(12, `0`)}`
const I1A = id(`1a`) // u1 phone
const I1B = id(`1b`) // u1 browser
const I1R = id(`1f`) // u1, revoked
const I1D = id(`1d`) // u1 desktop on mac-1
const I2 = id(`2a`) // u2 phone
const I3D = id(`3d`) // u3 daemon on box-3 (shared into team A)
const I4 = id(`4a`) // u4, team B only
const D1 = id(`d1`)
const D3 = id(`d3`)
const R1 = id(`f1`) // u1's private thread on mac-1
const R2 = id(`f2`) // u2's shared thread on u3's box
const C2 = id(`c2`) // its child
const R3 = id(`f3`) // team B
const R4 = id(`f4`) // keyless, team A, shared

const WRAP = Buffer.alloc(81, 7).toString(`base64url`)
const PUBLIC = generateTreeKey().publicKey

function install(rowId: string, userId: string, extra: Record<string, unknown> = {}) {
  return { id: rowId, userId, kind: `ios`, deviceId: null, x25519Public: `x-${rowId.slice(-2)}`, revokedAt: null, ...extra }
}

function session(rowId: string, extra: Record<string, unknown>) {
  return { id: rowId, rootId: rowId, parentId: null, shared: false, sealPublicKey: null, sealEpoch: null, ...extra }
}

function caller(userId: string) {
  return sessionKeysRouter.createCaller({
    session: { user: { id: userId, name: userId, email: `${userId}@example.com` }, session: { id: `s-${userId}` } },
    db,
    request: new Request(`http://localhost/`),
  } as never)
}

beforeEach(() => {
  db = createFakeDb({
    team_members: [
      { teamId: TEAM_A, userId: `u1` },
      { teamId: TEAM_A, userId: `u2` },
      { teamId: TEAM_A, userId: `u3` },
      { teamId: TEAM_B, userId: `u4` },
    ],
    devices: [
      { id: D1, userId: `u1`, deviceId: `mac-1`, sharedTeamIds: [] },
      { id: D3, userId: `u3`, deviceId: `box-3`, sharedTeamIds: [TEAM_A] },
    ],
    installs: [
      install(I1A, `u1`),
      install(I1B, `u1`, { kind: `web` }),
      install(I1R, `u1`, { revokedAt: new Date() }),
      install(I1D, `u1`, { kind: `desktop`, deviceId: D1 }),
      install(I2, `u2`),
      install(I3D, `u3`, { kind: `daemon`, deviceId: D3 }),
      install(I4, `u4`),
    ],
    sessions: [
      session(R1, { teamId: TEAM_A, userId: `u1`, deviceId: `mac-1` }),
      session(R2, { teamId: TEAM_A, userId: `u2`, deviceId: `box-3`, shared: true }),
      session(C2, { rootId: R2, parentId: R2, teamId: TEAM_A, userId: `u2`, deviceId: `box-3`, shared: true }),
      session(R3, { teamId: TEAM_B, userId: `u4`, deviceId: `box-4` }),
      session(R4, { teamId: TEAM_A, userId: `u1`, deviceId: `mac-1`, shared: true }),
    ],
  })
})

const root = (rowId: string) => db.rows(`sessions`).find((row) => row.id === rowId) as never
const readerIds = async (rowId: string) => [...(await treeReaders(db, root(rowId))).keys()].sort()

describe(`the readers rule`, () => {
  it(`private tree: the starter's live installs + the executing device`, async () => {
    const readers = await treeReaders(db, root(R1))
    expect([...readers.keys()].sort()).toEqual([I1A, I1B, I1D].sort())
    expect(readers.get(I1D)?.executing).toBe(true)
    expect(readers.get(I1A)?.executing).toBe(false)
  })

  it(`shared tree: every member's installs, the host box's install executing; never another team`, async () => {
    const readers = await treeReaders(db, root(R2))
    expect([...readers.keys()].sort()).toEqual([I1A, I1B, I1D, I2, I3D].sort())
    expect(readers.get(I3D)?.executing).toBe(true)
    expect(readers.has(I4)).toBe(false)
    expect(readers.has(I1R)).toBe(false)
    expect(await readerIds(R3)).toEqual([I4])
  })

  it(`unshared: the starter + the device; a member who left reads nothing`, async () => {
    ;(db.rows(`sessions`).find((row) => row.id === R2) as Record<string, unknown>).shared = false
    expect(await readerIds(R2)).toEqual([I2, I3D].sort())
    db.rows(`team_members`).splice(
      db.rows(`team_members`).findIndex((row) => row.userId === `u3`),
      1
    )
    expect(await readerIds(R2)).toEqual([I2])
  })
})

describe(`sessionKeys.create`, () => {
  it(`stores a tree's first key with the caller's own wrap`, async () => {
    const out = await caller(`u1`).create({
      rootId: R1,
      installId: I1A,
      epoch: 1,
      publicKey: PUBLIC,
      wraps: [
        { installId: I1A, wrapped: WRAP },
        { installId: I1B, wrapped: WRAP },
      ],
    })
    expect(out).toEqual({ rootId: R1, epoch: 1, wraps: 2, txid: 42 })
    expect(root(R1)).toMatchObject({ sealPublicKey: PUBLIC, sealEpoch: 1 })
    expect(db.rows(`session_keys`)).toMatchObject([
      { rootId: R1, epoch: 1, installId: I1A, wrapped: WRAP, createdByInstallId: I1A },
      { rootId: R1, epoch: 1, installId: I1B, createdByInstallId: I1A },
    ])
  })

  it(`refuses a missing own wrap, a wrong first epoch, a non-reader wrap and duplicate wraps`, async () => {
    const base = { rootId: R1, installId: I1A, epoch: 1, publicKey: PUBLIC }
    await expect(caller(`u1`).create({ ...base, wraps: [{ installId: I1B, wrapped: WRAP }] })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
    })
    await expect(caller(`u1`).create({ ...base, epoch: 2, wraps: [{ installId: I1A, wrapped: WRAP }] })).rejects.toMatchObject({
      code: `CONFLICT`,
    })
    await expect(
      caller(`u1`).create({ ...base, wraps: [{ installId: I1A, wrapped: WRAP }, { installId: I2, wrapped: WRAP }] })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      caller(`u1`).create({ ...base, wraps: [{ installId: I1A, wrapped: WRAP }, { installId: I1A, wrapped: WRAP }] })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      caller(`u1`).create({ ...base, wraps: [{ installId: I1A, wrapped: `short` }] })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(db.rows(`session_keys`)).toEqual([])
    expect(root(R1)).toMatchObject({ sealEpoch: null })
  })

  it(`needs the caller's own live install and a reader's seat`, async () => {
    const wraps = (installId: string) => [{ installId, wrapped: WRAP }]
    const base = { rootId: R1, epoch: 1, publicKey: PUBLIC }
    await expect(caller(`u1`).create({ ...base, installId: I1R, wraps: wraps(I1R) })).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(caller(`u1`).create({ ...base, installId: I2, wraps: wraps(I2) })).rejects.toMatchObject({ code: `FORBIDDEN` })
    // u2 is a teammate but R1 is u1's private thread.
    await expect(caller(`u2`).create({ ...base, installId: I2, wraps: wraps(I2) })).rejects.toMatchObject({ code: `NOT_FOUND` })
    // Another team's member.
    await expect(caller(`u4`).create({ ...base, installId: I4, wraps: wraps(I4) })).rejects.toMatchObject({ code: `NOT_FOUND` })
    // A child is not a root.
    await expect(caller(`u2`).create({ ...base, rootId: C2, installId: I2, wraps: wraps(I2) })).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
  })

  it(`rotates only to seal_epoch + 1, by the starter or the executing device`, async () => {
    await caller(`u2`).create({ rootId: R2, installId: I2, epoch: 1, publicKey: PUBLIC, wraps: [{ installId: I2, wrapped: WRAP }] })
    const next = generateTreeKey().publicKey
    // u1 reads the shared tree but neither started it nor runs it.
    await expect(
      caller(`u1`).create({ rootId: R2, installId: I1A, epoch: 2, publicKey: next, wraps: [{ installId: I1A, wrapped: WRAP }] })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(
      caller(`u2`).create({ rootId: R2, installId: I2, epoch: 3, publicKey: next, wraps: [{ installId: I2, wrapped: WRAP }] })
    ).rejects.toMatchObject({ code: `CONFLICT` })
    await expect(
      caller(`u2`).create({ rootId: R2, installId: I2, epoch: 1, publicKey: next, wraps: [{ installId: I2, wrapped: WRAP }] })
    ).rejects.toMatchObject({ code: `CONFLICT` })
    // The box running the tree (u3's daemon) may rotate.
    await caller(`u3`).create({ rootId: R2, installId: I3D, epoch: 2, publicKey: next, wraps: [{ installId: I3D, wrapped: WRAP }] })
    expect(root(R2)).toMatchObject({ sealEpoch: 2, sealPublicKey: next })
    // And the starter.
    await caller(`u2`).create({ rootId: R2, installId: I2, epoch: 3, publicKey: PUBLIC, wraps: [{ installId: I2, wrapped: WRAP }] })
    expect(root(R2)).toMatchObject({ sealEpoch: 3 })
  })
})

describe(`sessionKeys.pending + wrap + mine`, () => {
  beforeEach(async () => {
    await caller(`u2`).create({ rootId: R2, installId: I2, epoch: 1, publicKey: PUBLIC, wraps: [{ installId: I2, wrapped: WRAP }] })
  })

  it(`lists the readers missing the current epoch to a holder, automatically`, async () => {
    const pairs = await caller(`u2`).pending({ installId: I2 })
    expect(pairs.map((p) => p.targetInstallId).sort()).toEqual([I1A, I1B, I1D, I3D].sort())
    expect(pairs.find((p) => p.targetInstallId === I3D)).toEqual({
      rootId: R2,
      epoch: 1,
      publicKey: PUBLIC,
      targetInstallId: I3D,
      targetX25519Public: `x-3d`,
    })
    expect(await caller(`u2`).pending({ installId: I2, limit: 2 })).toHaveLength(2)
    // A non-holder sees nothing; someone else's install is refused.
    expect(await caller(`u1`).pending({ installId: I1A })).toEqual([])
    await expect(caller(`u1`).pending({ installId: I2 })).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`writes only listed pairs, skips pairs already wrapped, and the new holder sees its wrap`, async () => {
    expect(await caller(`u2`).wrap({ installId: I2, wraps: [{ rootId: R2, epoch: 1, targetInstallId: I1A, wrapped: WRAP }] })).toEqual({
      written: 1,
      skipped: 0,
    })
    expect((await caller(`u2`).pending({ installId: I2 })).map((p) => p.targetInstallId)).not.toContain(I1A)
    expect(await caller(`u2`).wrap({ installId: I2, wraps: [{ rootId: R2, epoch: 1, targetInstallId: I1A, wrapped: WRAP }] })).toEqual({
      written: 0,
      skipped: 1,
    })
    // Another team's install, a revoked one, a stale epoch, a tree the caller does not hold.
    for (const bad of [
      { rootId: R2, epoch: 1, targetInstallId: I4, wrapped: WRAP },
      { rootId: R2, epoch: 1, targetInstallId: I1R, wrapped: WRAP },
      { rootId: R2, epoch: 2, targetInstallId: I1B, wrapped: WRAP },
      { rootId: R1, epoch: 1, targetInstallId: I1B, wrapped: WRAP },
      { rootId: C2, epoch: 1, targetInstallId: I1B, wrapped: WRAP },
    ]) {
      await expect(caller(`u2`).wrap({ installId: I2, wraps: [bad] })).rejects.toMatchObject({ code: `FORBIDDEN` })
    }
    // The whole batch is refused when one pair is not pending.
    await expect(
      caller(`u2`).wrap({
        installId: I2,
        wraps: [
          { rootId: R2, epoch: 1, targetInstallId: I1B, wrapped: WRAP },
          { rootId: R2, epoch: 1, targetInstallId: I4, wrapped: WRAP },
        ],
      })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    expect(db.rows(`session_keys`).some((row) => row.installId === I1B)).toBe(false)

    // u1's phone now holds epoch 1 and keeps answering pending itself.
    expect(await caller(`u1`).mine({ installId: I1A })).toEqual([{ rootId: R2, epoch: 1, wrapped: WRAP }])
    expect(await caller(`u1`).mine({ installId: I1A, rootIds: [R1] })).toEqual([])
    expect((await caller(`u1`).pending({ installId: I1A })).map((p) => p.targetInstallId).sort()).toEqual([I1B, I1D, I3D].sort())
    await expect(caller(`u2`).mine({ installId: I1A })).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`a holder that is no longer a reader stops seeing pending pairs`, async () => {
    await caller(`u2`).wrap({ installId: I2, wraps: [{ rootId: R2, epoch: 1, targetInstallId: I1A, wrapped: WRAP }] })
    ;(db.rows(`sessions`).find((row) => row.id === R2) as Record<string, unknown>).shared = false
    expect(await caller(`u1`).pending({ installId: I1A })).toEqual([])
    await expect(
      caller(`u1`).wrap({ installId: I1A, wraps: [{ rootId: R2, epoch: 1, targetInstallId: I3D, wrapped: WRAP }] })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`a real wrap round-trips through the fixture install key`, async () => {
    const tree = generateTreeKey()
    const wrapped = wrapTreeKey(fixture.install.x25519Public, tree.secret)
    await caller(`u2`).wrap({ installId: I2, wraps: [{ rootId: R2, epoch: 1, targetInstallId: I1B, wrapped }] })
    const [mine] = await caller(`u1`).mine({ installId: I1B })
    expect(unwrapTreeKey(Buffer.from(fixture.install.secret, `hex`), mine!.wrapped)).toEqual(tree.secret)
  })
})

describe(`sessionKeys.publicKeys`, () => {
  beforeEach(async () => {
    await caller(`u1`).create({ rootId: R1, installId: I1A, epoch: 1, publicKey: PUBLIC, wraps: [{ installId: I1A, wrapped: WRAP }] })
    await caller(`u2`).create({ rootId: R2, installId: I2, epoch: 1, publicKey: PUBLIC, wraps: [{ installId: I2, wrapped: WRAP }] })
    await caller(`u4`).create({ rootId: R3, installId: I4, epoch: 1, publicKey: PUBLIC, wraps: [{ installId: I4, wrapped: WRAP }] })
  })

  it(`returns keyed roots the caller may read or send to, nothing else`, async () => {
    const all = [R1, R2, R3, R4, C2]
    const ids = async (userId: string) => (await caller(userId).publicKeys({ rootIds: all })).map((k) => k.rootId).sort()
    expect(await ids(`u1`)).toEqual([R1, R2].sort())
    expect(await ids(`u2`)).toEqual([R2])
    expect(await ids(`u3`)).toEqual([R2])
    expect(await ids(`u4`)).toEqual([R3])
    expect(await caller(`u2`).publicKeys({ rootIds: [R2] })).toEqual([{ rootId: R2, epoch: 1, publicKey: PUBLIC }])
  })
})
