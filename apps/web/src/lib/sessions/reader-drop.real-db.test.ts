// @vitest-environment node
//
// #EXP-1951: a tree's key rotates when a reader is dropped. The ATTACK, through
// the routers against the REAL migrated Postgres with real keys: a dropped
// reader (a revoked install, a removed member) kept every secret it ever
// unwrapped, and the server double here is MALICIOUS: it hands the attacker
// every `session_keys` row of the tree, whoever it was wrapped for. What the
// next message is sealed to must still not open.
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { devices, installs, sessionKeys, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
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
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'sessions' AND column_name = 'seal_rotate_epoch') AS ok
`)
if (!ready.ok) console.info(`[reader-drop.real-db] skipped: ${ready.reason}`)

const { sessionKeysRouter } = await import(`@/lib/trpc/session-keys`)
const { installsRouter } = await import(`@/lib/trpc/installs`)
const { teamMembersRouter } = await import(`@/lib/trpc/team-members`)

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
