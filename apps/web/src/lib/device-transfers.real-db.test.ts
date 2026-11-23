// @vitest-environment node
import { describe, expect, it } from "vitest"
import { eq, sql } from "drizzle-orm"
import { deviceCommands, devices, deviceTransfers, installs, messages, sessions, teamMembers, teams } from "@/db/schema"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"

// #FEED-295: device-to-device transfers against the REAL migrated Postgres,
// inside a rolled-back transaction: the same-owner rule, the two queued
// commands, the log row, progress, the end (stale commands failed, the
// asking session woken) and cancel.

const ready = await probeRealDb(`SELECT to_regclass('public.device_transfers') IS NOT NULL AS ok`)
if (!ready.ok) console.info(`[device-transfers.real-db] skipped: ${ready.reason}`)

const { devicesRouter } = await import(`@/lib/trpc/devices`)
const { FILE_TRANSFER_CAP } = await import(`@/lib/device-transfers`)

const id8 = () => crypto.randomUUID().slice(0, 8)

async function world(tx: RealDbTx) {
  const users: string[] = []
  for (let n = 0; n < 2; n++) {
    const userId = `feed295-${id8()}`
    await tx.execute(sql`
      INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
      VALUES (${userId}, 'Tester', ${`${userId}@feed295.test`}, true, false, now(), now())
    `)
    users.push(userId)
  }
  const [owner, mate] = users as [string, string]
  const [team] = await tx.insert(teams).values({ name: `T`, slug: `t-${id8()}` }).returning({ id: teams.id })
  await tx.insert(teamMembers).values(users.map((userId, n) => ({ teamId: team!.id, userId, role: n === 0 ? (`owner` as const) : (`member` as const) })))
  const [other] = await tx.insert(teams).values({ name: `O`, slug: `o-${id8()}` }).returning({ id: teams.id })
  const box = async (userId: string, name: string, caps: string[] = [FILE_TRANSFER_CAP], install = true, sharedTeamIds: string[] = []) => {
    const [row] = await tx
      .insert(devices)
      .values({ userId, deviceId: `${name}-${id8()}`, label: name, kind: `server`, caps, lastSeenAt: new Date(), sharedTeamIds })
      .returning({ id: devices.id, deviceId: devices.deviceId })
    if (install) {
      await tx.insert(installs).values({
        userId,
        kind: `daemon`,
        deviceId: row!.id,
        endpointId: `${name}-ep-${id8()}`,
        x25519Public: Buffer.alloc(32, name.length).toString(`base64url`),
      })
    }
    return row!.deviceId
  }
  const mint = await box(owner, `mint`)
  const deck = await box(owner, `steamdeck`)
  const old = await box(owner, `old`, [])
  const bare = await box(owner, `bare`, [FILE_TRANSFER_CAP], false)
  const theirs = await box(mate, `theirs`)
  const otherBox = await box(owner, `other-box`, [FILE_TRANSFER_CAP], true, [other!.id])
  const sessionId = crypto.randomUUID()
  await tx.insert(sessions).values({ id: sessionId, rootId: sessionId, teamId: team!.id, userId: owner, deviceId: mint } as never)
  const callerOf = (userId: string) =>
    devicesRouter.createCaller({
      session: { user: { id: userId }, session: { id: `s` } },
      db: tx,
      request: new Request(`http://x.test`),
    } as never)
  return { owner, mate, mint, deck, old, bare, theirs, otherBox, teamId: team!.id, otherTeamId: other!.id, sessionId, callerOf }
}

describe.skipIf(!ready.ok)(`device transfers (#FEED-295)`, () => {
  it(`runs only between the caller's own capable machines and queues one command per side`, async () => {
    await inRollback(async (tx) => {
      const w = await world(tx)
      const me = w.callerOf(w.owner)
      const base = { srcPath: `~/WoW/Sounds`, destPath: `/run/media/sd/WoW/Sounds` }
      await expect(me.startTransfer({ ...base, fromDeviceId: w.mint, toDeviceId: w.theirs })).rejects.toThrow(/your own/)
      await expect(w.callerOf(w.mate).startTransfer({ ...base, fromDeviceId: w.mint, toDeviceId: w.theirs })).rejects.toThrow(/your own/)
      await expect(me.startTransfer({ ...base, fromDeviceId: w.mint, toDeviceId: w.mint })).rejects.toThrow(/same device/)
      await expect(me.startTransfer({ ...base, fromDeviceId: w.mint, toDeviceId: w.old })).rejects.toThrow(/without file transfers/)
      await expect(me.startTransfer({ ...base, fromDeviceId: w.bare, toDeviceId: w.deck })).rejects.toThrow(/eNet node/)
      await expect(me.startTransfer({ ...base, destPath: `/a/../b`, fromDeviceId: w.mint, toDeviceId: w.deck })).rejects.toThrow(/\.\./)
      await expect(me.startTransfer({ ...base, srcPath: `relative/x`, fromDeviceId: w.mint, toDeviceId: w.deck })).rejects.toThrow(/absolute/)
      // #EXP-1440: an own machine shared only with ANOTHER team is that team's for a session of this one.
      await expect(me.startTransfer({ ...base, fromDeviceId: w.otherBox, toDeviceId: w.deck, teamId: w.teamId })).rejects.toThrow(/another team/)
      await expect(me.startTransfer({ ...base, fromDeviceId: w.deck, toDeviceId: w.otherBox, teamId: w.otherTeamId })).resolves.toMatchObject({ status: `queued` })

      const started = await me.startTransfer({ ...base, fromDeviceId: w.mint, toDeviceId: w.deck, sessionId: w.sessionId })
      expect(started).toMatchObject({ status: `queued`, fromDeviceId: w.mint, toDeviceId: w.deck, fromOnline: true, toOnline: true })
      const commands = await tx
        .select({ kind: deviceCommands.kind, payload: deviceCommands.payload, status: deviceCommands.status })
        .from(deviceCommands)
        .where(sql`${deviceCommands.payload}->>'transferId' = ${started.transferId}`)
      expect(commands.map((c) => c.kind).sort()).toEqual([`file_receive`, `file_send`])
      const send = commands.find((c) => c.kind === `file_send`)!
      const receive = commands.find((c) => c.kind === `file_receive`)!
      expect(send.payload).toMatchObject({ srcPath: base.srcPath, peerDeviceId: w.deck })
      expect(receive.payload).toMatchObject({ destPath: base.destPath, peerDeviceId: w.mint })
      // Each side dials only the other's endpoint.
      expect(send.payload.peer).toMatch(/^steamdeck-ep-/)
      expect(receive.payload.peer).toMatch(/^mint-ep-/)
      // Another person never reads it.
      await expect(w.callerOf(w.mate).transfer({ transferId: started.transferId })).rejects.toThrow(/not found/)
      await expect(w.callerOf(w.mate).reportTransfer({ transferId: started.transferId, role: `receive`, status: `failed` })).rejects.toThrow(/not found/)
    })
  })

  it(`logs progress, ends once, fails the stale commands and wakes the asking session`, async () => {
    await inRollback(async (tx) => {
      const w = await world(tx)
      const me = w.callerOf(w.owner)
      const { transferId } = await me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.deck, srcPath: `~/a`, destPath: `~/b`, sessionId: w.sessionId })
      // The source's own heartbeat carries no counters; the destination's does.
      expect(await me.reportTransfer({ transferId, role: `send`, status: `running`, bytesDone: 999 })).toEqual({ ok: true, status: `running` })
      await me.reportTransfer({ transferId, role: `receive`, status: `running`, files: 3, filesDone: 1, bytes: 6_000_000_000, bytesDone: 2_000_000_000 })
      let row = await me.transfer({ transferId })
      expect(row).toMatchObject({ status: `running`, files: 3, filesDone: 1, bytes: 6_000_000_000, bytesDone: 2_000_000_000 })
      expect(row.startedAt).not.toBeNull()
      // Only the destination can say done.
      expect(await me.reportTransfer({ transferId, role: `send`, status: `done` })).toEqual({ ok: true, status: `running` })
      // The source command is still pending here (it completes itself); the end fails any pending one.
      expect(
        await me.reportTransfer({ transferId, role: `receive`, status: `done`, files: 3, filesDone: 3, bytes: 6_000_000_000, bytesDone: 6_000_000_000, skipped: [`link`] })
      ).toEqual({ ok: true, status: `done` })
      row = await me.transfer({ transferId })
      expect(row).toMatchObject({ status: `done`, filesDone: 3, skipped: [`link`], error: null })
      expect(row.completedAt).not.toBeNull()
      const closed = await tx
        .select({ status: deviceCommands.status, result: deviceCommands.result })
        .from(deviceCommands)
        .where(sql`${deviceCommands.payload}->>'transferId' = ${transferId}`)
      expect(closed).toEqual([
        { status: `done`, result: `Transfer done.` },
        { status: `done`, result: `Transfer done.` },
      ])
      // Terminal once: a late report changes nothing and tells the machine.
      expect(await me.reportTransfer({ transferId, role: `receive`, status: `failed`, error: `late` })).toEqual({ ok: false, status: `done` })
      const [wake] = await tx.select().from(messages).where(eq(messages.sessionId, w.sessionId))
      expect(wake!.body).toBe(`Transfer ${transferId} done: mint:~/a → steamdeck:~/b, 3 files, 6.0 GB, every checksum verified, 1 skipped (symlinks or protected paths).`)
      expect(wake!.consumedAt).toBeNull()
      expect(wake!.card).toMatchObject({ kind: `event`, event: `watch_fired`, watchId: transferId })
      const list = await me.transfers({ limit: 5 })
      expect(list.map((t) => t.transferId)).toEqual([transferId])
    })
  })

  it(`cancel ends it for both machines; a failure carries its reason`, async () => {
    await inRollback(async (tx) => {
      const w = await world(tx)
      const me = w.callerOf(w.owner)
      const a = await me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.deck, srcPath: `/x`, destPath: `/y` })
      const cancelled = await me.cancelTransfer({ transferId: a.transferId })
      expect(cancelled.status).toBe(`cancelled`)
      const left = await tx
        .select({ status: deviceCommands.status, result: deviceCommands.result })
        .from(deviceCommands)
        .where(sql`${deviceCommands.payload}->>'transferId' = ${a.transferId}`)
      expect(left).toEqual([
        { status: `failed`, result: `Transfer cancelled.` },
        { status: `failed`, result: `Transfer cancelled.` },
      ])
      expect(await me.reportTransfer({ transferId: a.transferId, role: `receive`, status: `running` })).toEqual({ ok: false, status: `cancelled` })
      const b = await me.startTransfer({ fromDeviceId: w.deck, toDeviceId: w.mint, srcPath: `~/.ssh`, destPath: `/y2` })
      await me.reportTransfer({ transferId: b.transferId, role: `send`, status: `failed`, error: `source: /home/u/.ssh is protected (/home/u/.ssh)` })
      const [row] = await tx.select().from(deviceTransfers).where(eq(deviceTransfers.id, b.transferId))
      expect(row).toMatchObject({ status: `failed`, error: `source: /home/u/.ssh is protected (/home/u/.ssh)` })
    })
  })
})
