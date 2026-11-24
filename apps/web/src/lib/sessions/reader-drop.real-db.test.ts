// @vitest-environment node
//
// #EXP-1951: a tree's key rotates when a reader is dropped. The ATTACK, through
// the routers against the REAL migrated Postgres with real keys: a dropped
// reader (a revoked install, a removed member) kept every secret it ever
// unwrapped, and the server double here is MALICIOUS: it hands the attacker
// every `session_keys` row of the tree, whoever it was wrapped for. What the
// next message is sealed to must still not open.
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, inArray, sql } from "drizzle-orm"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"
import { db } from "@/db/connection"
import { devices, installs, sessionKeys, sessions, teamMembers, teams } from "@/db/schema"
import { inRollback, probeRealDb } from "@/test/real-db"
import { generateTreeKey, openBody, sealBody, unwrapTreeKey, wrapTreeKey } from "@exp/domain-contract/sealed-content"

process.env.BETTER_AUTH_SECRET ||= `reader-drop-test-secret-0123456789`

vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => null,
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
  notifySessionAsk: async () => {},
  notifyDeviceShareRemoved: async () => {},
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'sessions' AND column_name = 'seal_rotate_epoch') AS ok
`)
if (!ready.ok) console.info(`[reader-drop.real-db] skipped: ${ready.reason}`)

const { sessionKeysRouter } = await import(`@/lib/trpc/session-keys`)
const { installsRouter } = await import(`@/lib/trpc/installs`)
const { teamMembersRouter } = await import(`@/lib/trpc/team-members`)
const { devicesRouter } = await import(`@/lib/trpc/devices`)
const { usersRouter } = await import(`@/lib/trpc/users`)
const { dropNonReaderKeysInTx, dropReaderKeysInTx, DROP_CHUNK } = await import(`@/lib/sessions/tree-keys`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const ctx = (userId: string) => ({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) }) as never
const keys = (userId: string) => sessionKeysRouter.createCaller(ctx(userId))

async function user(): Promise<string> {
  const id = `drop-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@drop.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function team(members: Record<string, `owner` | `member`>): Promise<string> {
  const [row] = await db.insert(teams).values({ name: `Drop`, slug: `drop-${id8()}` }).returning({ id: teams.id })
  made.teams.push(row!.id)
  await db.insert(teamMembers).values(Object.entries(members).map(([userId, role]) => ({ teamId: row!.id, userId, role })))
  return row!.id
}

/** A client install: its X25519 keypair and every tree secret it unwrapped. */
type Install = { id: string; userId: string; secret: Uint8Array; publicKey: string; kept: Map<string, Map<number, Uint8Array>> }

async function install(userId: string, kind: string, deviceRowId: string | null = null): Promise<Install> {
  const key = generateTreeKey()
  const [row] = await db
    .insert(installs)
    .values({ userId, kind, deviceId: deviceRowId, endpointId: `ep-${crypto.randomUUID()}`, x25519Public: key.publicKey })
    .returning({ id: installs.id })
  return { id: row!.id, userId, secret: key.secret, publicKey: key.publicKey, kept: new Map() }
}

const keep = (who: Install, rootId: string, epoch: number, secret: Uint8Array) => {
  const epochs = who.kept.get(rootId) ?? new Map<number, Uint8Array>()
  epochs.set(epoch, secret)
  who.kept.set(rootId, epochs)
}

/** `sessionKeys.mine` as an honest server answers it. */
async function pull(who: Install) {
  for (const row of await keys(who.userId).mine({ installId: who.id })) keep(who, row.rootId, row.epoch, unwrapTreeKey(who.secret, row.wrapped))
}

/** The MALICIOUS server: every wrap row of the tree goes to the attacker,
 *  revoked or removed or not; it keeps what its install secret opens. */
async function leakEveryWrap(attacker: Install, rootId: string) {
  for (const row of await db.select().from(sessionKeys).where(eq(sessionKeys.rootId, rootId))) {
    try {
      keep(attacker, rootId, row.epoch, unwrapTreeKey(attacker.secret, row.wrapped))
    } catch {
      // wrapped for another install
    }
  }
}

const rootRow = async (rootId: string) => (await db.select().from(sessions).where(eq(sessions.id, rootId)))[0]!
const wrapRows = async (rootId: string) => db.select().from(sessionKeys).where(eq(sessionKeys.rootId, rootId))

/** One holder pass of an honest install, as the desktop runs it (pull,
 *  rotate a root whose rotation is due, answer pending). */
async function holderPass(who: Install, rootId: string) {
  await pull(who)
  const root = await rootRow(rootId)
  if (root.sealRotateEpoch !== null && root.sealEpoch === root.sealRotateEpoch) {
    const next = generateTreeKey()
    await keys(who.userId)
      .create({ rootId, installId: who.id, epoch: root.sealEpoch + 1, publicKey: next.publicKey, wraps: [{ installId: who.id, wrapped: wrapTreeKey(who.publicKey, next.secret) }] })
      .then(() => keep(who, rootId, root.sealEpoch! + 1, next.secret))
      .catch(() => {})
  }
  const pending = await keys(who.userId).pending({ installId: who.id })
  const wraps = pending.flatMap((pair) => {
    const secret = who.kept.get(pair.rootId)?.get(pair.epoch)
    return secret ? [{ rootId: pair.rootId, epoch: pair.epoch, targetInstallId: pair.targetInstallId, wrapped: wrapTreeKey(pair.targetX25519Public, secret) }] : []
  })
  if (wraps.length > 0) await keys(who.userId).wrap({ installId: who.id, wraps })
  return wraps.map((wrap) => wrap.targetInstallId)
}

/** The next message: a writer seals to the key the server names as current. */
async function nextMessage(writer: string, rootId: string, text: string) {
  const [current] = await keys(writer).publicKeys({ rootIds: [rootId] })
  return sealBody(current!.publicKey, current!.epoch, text)
}

/** A shared chat of a team: its starter (a desktop on their device, a phone)
 *  and a teammate (a browser), every install holding epoch 1. */
async function world() {
  const starter = await user()
  const mate = await user()
  const teamId = await team({ [starter]: `owner`, [mate]: `member` })
  const machine = `dev-${id8()}`
  const [box] = await db.insert(devices).values({ userId: starter, deviceId: machine, label: machine, kind: `desktop` }).returning({ id: devices.id })
  const desktop = await install(starter, `desktop`, box!.id)
  const phone = await install(starter, `ios`)
  const browser = await install(mate, `web`)
  const rootId = crypto.randomUUID()
  await db.insert(sessions).values({ id: rootId, rootId, teamId, userId: starter, deviceId: machine, shared: true } as never)
  const tree = generateTreeKey()
  await keys(starter).create({
    rootId,
    installId: desktop.id,
    epoch: 1,
    publicKey: tree.publicKey,
    wraps: [desktop, phone, browser].map((to) => ({ installId: to.id, wrapped: wrapTreeKey(to.publicKey, tree.secret) })),
  })
  for (const who of [desktop, phone, browser]) await pull(who)
  const [member] = await db.select().from(teamMembers).where(eq(teamMembers.userId, mate))
  return { starter, mate, teamId, machine, desktop, phone, browser, rootId, mateMemberId: member!.id }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`#EXP-1951 a dropped reader opens nothing sealed after the drop`, () => {
  it(`a revoked install: the next message is sealed to an epoch it holds no secret of, and the server has no wrap to leak`, async () => {
    const w = await world()
    expect(openBody(await nextMessage(w.mate, w.rootId, `before`), w.phone.kept.get(w.rootId)!).status).toBe(`opened`)

    await installsRouter.createCaller(ctx(w.starter)).revoke({ installId: w.phone.id })
    // In the same transaction: its wraps are gone and the root is due.
    expect((await wrapRows(w.rootId)).map((row) => row.installId).sort()).toEqual([w.desktop.id, w.browser.id].sort())
    expect(await rootRow(w.rootId)).toMatchObject({ sealEpoch: 1, sealRotateEpoch: 1 })

    // The next online holder rotates; the teammate's browser gets the new epoch.
    expect(await holderPass(w.desktop, w.rootId)).toEqual([w.browser.id])
    expect(await rootRow(w.rootId)).toMatchObject({ sealEpoch: 2, sealRotateEpoch: null })
    const body = await nextMessage(w.mate, w.rootId, `after the phone was lost`)

    await leakEveryWrap(w.phone, w.rootId)
    expect([...w.phone.kept.get(w.rootId)!.keys()]).toEqual([1])
    expect(openBody(body, w.phone.kept.get(w.rootId)!)).toMatchObject({ status: `noKey`, epoch: 2 })
    // The readers that stayed read on.
    await pull(w.browser)
    expect(openBody(body, w.browser.kept.get(w.rootId)!)).toMatchObject({ status: `opened`, text: `after the phone was lost` })
    expect(openBody(body, w.desktop.kept.get(w.rootId)!).status).toBe(`opened`)
  })

  it(`a member removed from the team on a shared chat: the same, and \`mine\` serves them nothing of it`, async () => {
    const w = await world()
    await teamMembersRouter.createCaller(ctx(w.starter)).remove({ memberId: w.mateMemberId })
    expect((await wrapRows(w.rootId)).map((row) => row.installId).sort()).toEqual([w.desktop.id, w.phone.id].sort())
    expect(await rootRow(w.rootId)).toMatchObject({ sealEpoch: 1, sealRotateEpoch: 1 })
    // Their install is not revoked: `mine` answers, with no row of the tree.
    expect(await keys(w.mate).mine({ installId: w.browser.id, rootIds: [w.rootId] })).toEqual([])

    expect(await holderPass(w.desktop, w.rootId)).toEqual([w.phone.id])
    const body = await nextMessage(w.starter, w.rootId, `after they left`)
    await leakEveryWrap(w.browser, w.rootId)
    expect(openBody(body, w.browser.kept.get(w.rootId)!)).toMatchObject({ status: `noKey`, epoch: 2 })
    await pull(w.phone)
    expect(openBody(body, w.phone.kept.get(w.rootId)!).status).toBe(`opened`)
  })
})

// ── sealed-content.json `rotation.dropCases`: every drop, through its router ──

/** The fixture's world: the starter's chat on the host's shared machine. */
async function dropWorld(tree: string) {
  const [starter, mate, host] = [await user(), await user(), await user()]
  const teamId = await team({ [starter]: `owner`, [mate]: `member`, [host]: `member` })
  const machine = `box-${id8()}`
  const [box] = await db
    .insert(devices)
    .values({ userId: host, deviceId: machine, label: machine, kind: `server`, sharedTeamIds: [teamId] })
    .returning({ id: devices.id })
  const installsOf = {
    starterPhone: await install(starter, `ios`),
    mateBrowser: await install(mate, `web`),
    hostDaemon: await install(host, `daemon`, box!.id),
    hostPhone: await install(host, `ios`),
  }
  const rootId = crypto.randomUUID()
  await db.insert(sessions).values({ id: rootId, rootId, teamId, userId: starter, deviceId: machine, shared: tree === `shared` } as never)
  const key = generateTreeKey()
  const readers = tree === `shared` ? Object.values(installsOf) : [installsOf.starterPhone, installsOf.hostDaemon]
  await keys(starter).create({
    rootId,
    installId: installsOf.starterPhone.id,
    epoch: 1,
    publicKey: key.publicKey,
    wraps: readers.map((to) => ({ installId: to.id, wrapped: wrapTreeKey(to.publicKey, key.secret) })),
  })
  const memberId = async (userId: string) =>
    (await db.select().from(teamMembers).where(and(eq(teamMembers.teamId, teamId), eq(teamMembers.userId, userId))))[0]!.id
  return { starter, mate, host, teamId, machine, boxId: box!.id, installsOf, rootId, readers, memberId }
}

type DropWorld = Awaited<ReturnType<typeof dropWorld>>
const EVENTS: Record<string, (w: DropWorld) => Promise<unknown>> = {
  installRevoked: (w) => installsRouter.createCaller(ctx(w.starter)).revoke({ installId: w.installsOf.starterPhone.id }),
  memberRemoved: async (w) => teamMembersRouter.createCaller(ctx(w.starter)).remove({ memberId: await w.memberId(w.mate) }),
  memberLeft: async (w) => teamMembersRouter.createCaller(ctx(w.mate)).remove({ memberId: await w.memberId(w.mate) }),
  accountDeleted: (w) => usersRouter.createCaller(ctx(w.mate)).deleteAccount({ confirm: true }),
  hostRemoved: async (w) => teamMembersRouter.createCaller(ctx(w.starter)).remove({ memberId: await w.memberId(w.host) }),
  deviceUnshared: (w) => devicesRouter.createCaller(ctx(w.host)).setShared({ deviceId: w.machine, teamId: w.teamId, shared: false }),
  shareDropped: (w) => devicesRouter.createCaller(ctx(w.starter)).dropShare({ id: w.boxId, teamId: w.teamId }),
  deviceRemoved: (w) => devicesRouter.createCaller(ctx(w.host)).remove({ deviceId: w.machine }),
  treePrivate: (w) =>
    db.transaction(async (tx) => {
      await tx.update(sessions).set({ shared: false }).where(eq(sessions.id, w.rootId))
      await dropNonReaderKeysInTx(tx, [w.rootId])
    }),
}

describe.skipIf(!ready.ok)(`#EXP-1951 rotation.dropCases: each drop deletes the dropped wraps and flags the root, in its transaction`, () => {
  for (const c of fixture.rotation.dropCases) {
    it(`${c.event}, ${c.tree}: ${c.name}`, async () => {
      const w = await dropWorld(c.tree)
      const nameOf = (installId: string) => Object.entries(w.installsOf).find(([, row]) => row.id === installId)![0]
      const before = w.readers.map((row) => nameOf(row.id)).sort()
      await EVENTS[c.event]!(w)
      const kept = (await wrapRows(w.rootId)).map((row) => nameOf(row.installId)).sort()
      expect(kept).toEqual(before.filter((name) => !c.drops.includes(name)))
      expect((await rootRow(w.rootId)).sealRotateEpoch).toBe(c.due ? 1 : null)
      // What stays is what `pending` would wrap for: no dropped install is a reader.
      if (c.event !== `accountDeleted`) {
        const holder = w.installsOf.starterPhone
        if (kept.includes(`starterPhone`)) {
          const due = await keys(w.starter).due({ installId: holder.id })
          expect(due).toEqual(c.due ? [{ rootId: w.rootId, epoch: 1 }] : [])
        }
      }
    })
  }
})

describe.skipIf(!ready.ok)(`#EXP-1951 rotation.rotatorCases: who may create the next epoch`, () => {
  for (const c of fixture.rotation.rotatorCases) {
    it(c.name, async () => {
      const w = await dropWorld(`shared`)
      // `dropped` = the starter's revoked phone; `removed` = the mate after their removal.
      if (c.due) {
        await installsRouter.createCaller(ctx(w.starter)).revoke({ installId: w.installsOf.starterPhone.id })
        if (c.caller === `removed`) await teamMembersRouter.createCaller(ctx(w.starter)).remove({ memberId: await w.memberId(w.mate) })
      }
      expect((await rootRow(w.rootId)).sealRotateEpoch).toBe(c.due ? 1 : null)
      const second = c.due ? await install(w.starter, `web`) : w.installsOf.starterPhone
      const who = {
        starter: second,
        executing: w.installsOf.hostDaemon,
        reader: w.installsOf.hostPhone,
        dropped: w.installsOf.starterPhone,
        removed: w.installsOf.mateBrowser,
      }[c.caller]!
      const next = generateTreeKey()
      const create = keys(who.userId).create({ rootId: w.rootId, installId: who.id, epoch: 2, publicKey: next.publicKey, wraps: [{ installId: who.id, wrapped: wrapTreeKey(who.publicKey, next.secret) }] })
      if (c.allowed) {
        await create
        expect(await rootRow(w.rootId)).toMatchObject({ sealEpoch: 2, sealRotateEpoch: null })
      } else {
        await expect(create).rejects.toMatchObject({ code: (c as { code?: string }).code })
        expect((await rootRow(w.rootId)).sealEpoch).toBe(1)
      }
    })
  }
})

describe.skipIf(!ready.ok)(`#EXP-1951 the server has nothing of the new epoch for a dropped reader`, () => {
  it(`a rotation never wraps for the dropped: create refuses them as a wrap target, pending never names them, wrap refuses the pair, due and mine answer nothing`, async () => {
    const w = await dropWorld(`shared`)
    const { starterPhone, mateBrowser, hostDaemon, hostPhone } = w.installsOf
    await teamMembersRouter.createCaller(ctx(w.starter)).remove({ memberId: await w.memberId(w.mate) })
    await installsRouter.createCaller(ctx(w.host)).revoke({ installId: hostPhone.id })
    const next = generateTreeKey()
    const wrapFor = (to: Install) => ({ installId: to.id, wrapped: wrapTreeKey(to.publicKey, next.secret) })
    // A holder that still thinks they read (a stale list, or a lying client) is refused whole.
    for (const dropped of [mateBrowser, hostPhone]) {
      await expect(
        keys(w.starter).create({ rootId: w.rootId, installId: starterPhone.id, epoch: 2, publicKey: next.publicKey, wraps: [wrapFor(starterPhone), wrapFor(dropped)] })
      ).rejects.toMatchObject({ code: `BAD_REQUEST`, message: fixture.clearTree.wrapRefusal })
    }
    await keys(w.starter).create({ rootId: w.rootId, installId: starterPhone.id, epoch: 2, publicKey: next.publicKey, wraps: [wrapFor(starterPhone)] })
    const pending = await keys(w.starter).pending({ installId: starterPhone.id })
    expect(pending.map((pair) => pair.targetInstallId)).toEqual([hostDaemon.id])
    for (const dropped of [mateBrowser, hostPhone]) {
      await expect(
        keys(w.starter).wrap({ installId: starterPhone.id, wraps: [{ rootId: w.rootId, epoch: 2, targetInstallId: dropped.id, wrapped: wrapFor(dropped).wrapped }] })
      ).rejects.toMatchObject({ code: `FORBIDDEN` })
    }
    // The dropped ask: the removed member's live install gets nothing, the revoked one is refused.
    expect(await keys(w.mate).mine({ installId: mateBrowser.id })).toEqual([])
    expect(await keys(w.mate).pending({ installId: mateBrowser.id })).toEqual([])
    expect(await keys(w.mate).due({ installId: mateBrowser.id })).toEqual([])
    await expect(keys(w.host).mine({ installId: hostPhone.id })).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(keys(w.host).pending({ installId: hostPhone.id })).rejects.toMatchObject({ code: `FORBIDDEN` })
    // And a server that skips those checks has no row to hand out: none of epoch 2 is theirs.
    const rows = await wrapRows(w.rootId)
    expect(rows.filter((row) => row.epoch === 2).map((row) => row.installId)).toEqual([starterPhone.id])
    expect(rows.some((row) => row.installId === mateBrowser.id || row.installId === hostPhone.id)).toBe(false)
  })

  it(`a member re-invited after the rotation reads on from the new epoch (through pending), like any new reader`, async () => {
    const w = await world()
    await teamMembersRouter.createCaller(ctx(w.starter)).remove({ memberId: w.mateMemberId })
    await holderPass(w.desktop, w.rootId)
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: w.mate, role: `member` })
    expect(await holderPass(w.desktop, w.rootId)).toEqual([w.browser.id])
    await pull(w.browser)
    expect([...w.browser.kept.get(w.rootId)!.keys()].sort()).toEqual([1, 2])
  })
})

describe.skipIf(!ready.ok)(`#EXP-1951 the cost of a drop`, () => {
  it(`a member with thousands of trees: a fixed number of queries per ${DROP_CHUNK} roots, never some per root`, async () => {
    const ROOTS = Number(process.env.EXP1951_ROOTS ?? 1200)
    await inRollback(async (tx) => {
      const ids = [`cost-${id8()}`, `cost-${id8()}`]
      for (const userId of ids) {
        await tx.execute(sql`INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at) VALUES (${userId}, 'Tester', ${`${userId}@drop.test`}, true, false, now(), now())`)
      }
      const [teamRow] = await tx.insert(teams).values({ name: `Cost`, slug: `cost-${id8()}` }).returning({ id: teams.id })
      await tx.insert(teamMembers).values(ids.map((userId, n) => ({ teamId: teamRow!.id, userId, role: n === 0 ? (`owner` as const) : (`member` as const) })))
      const made = await tx
        .insert(installs)
        .values(ids.map((userId) => ({ userId, kind: `web`, endpointId: `ep-${crypto.randomUUID()}`, x25519Public: generateTreeKey().publicKey })))
        .returning({ id: installs.id })
      const roots = Array.from({ length: ROOTS }, () => crypto.randomUUID())
      const wrap = Buffer.alloc(81, 7).toString(`base64url`)
      for (let at = 0; at < roots.length; at += 500) {
        const chunk = roots.slice(at, at + 500)
        await tx.insert(sessions).values(chunk.map((id) => ({ id, rootId: id, teamId: teamRow!.id, userId: ids[0]!, deviceId: `cost-box`, shared: true, sealPublicKey: generateTreeKey().publicKey, sealEpoch: 1 })) as never)
        await tx.insert(sessionKeys).values(chunk.flatMap((rootId) => made.map((row) => ({ rootId, epoch: 1, installId: row.id, wrapped: wrap, createdByInstallId: made[0]!.id }))))
      }
      await tx.delete(teamMembers).where(and(eq(teamMembers.teamId, teamRow!.id), eq(teamMembers.userId, ids[1]!)))
      let queries = 0
      const counting = new Proxy(tx, {
        get(target, prop, receiver) {
          if (prop === `select` || prop === `update` || prop === `delete` || prop === `execute`) queries++
          const value = Reflect.get(target, prop, receiver)
          return typeof value === `function` ? value.bind(target) : value
        },
      })
      const started = performance.now()
      const flagged = await dropReaderKeysInTx(counting as never, { userId: ids[1]! }, [teamRow!.id])
      const took = performance.now() - started
      process.stderr.write(`[EXP-1951] dropping a reader of ${ROOTS} trees: ${took.toFixed(0)} ms, ${queries} queries\n`)
      expect(flagged).toHaveLength(ROOTS)
      expect(queries).toBeLessThanOrEqual(4 + 12 * Math.ceil(ROOTS / DROP_CHUNK))
      const left = await tx.select({ installId: sessionKeys.installId }).from(sessionKeys).where(inArray(sessionKeys.installId, made.map((row) => row.id)))
      expect(left.filter((row) => row.installId === made[1]!.id)).toHaveLength(0)
      expect(left.filter((row) => row.installId === made[0]!.id)).toHaveLength(ROOTS)
    })
  }, 120_000)
})
