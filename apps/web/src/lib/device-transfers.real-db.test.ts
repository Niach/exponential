// @vitest-environment node
import { describe, expect, it } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { deviceCommands, devices, deviceTransfers, installs, mcpTasks, messages, sessions, teamMembers, teams } from "@/db/schema"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"

// #FEED-295: device-to-device transfers against the REAL migrated Postgres,
// each case inside a rolled-back transaction: own devices only (another
// person's machine is refused), the team rule, the preconditions, the two
// queued commands, progress, the end (pending commands closed, the asking
// session woken, an outside host's MCP task ended) and cancel.

const ready = await probeRealDb(`SELECT to_regclass('public.device_transfers') IS NOT NULL AS ok`)
if (!ready.ok) console.info(`[device-transfers.real-db] skipped: ${ready.reason}`)

const { devicesRouter } = await import(`@/lib/trpc/devices`)
const { FILE_TRANSFER_CAP, TRANSFER_COPY, TRANSFERS_LIVE_MAX } = await import(`@/lib/device-transfers`)
const { inSession } = await import(`@/lib/sessions/messages`)

const id8 = () => crypto.randomUUID().slice(0, 8)

async function world(tx: RealDbTx) {
  const people: string[] = []
  for (let n = 0; n < 2; n++) {
    const userId = `feed295-${id8()}`
    await tx.execute(sql`
      INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
      VALUES (${userId}, 'Tester', ${`${userId}@feed295.test`}, true, false, now(), now())
    `)
    people.push(userId)
  }
  const [owner, mate] = people as [string, string]
  const [team] = await tx.insert(teams).values({ name: `T`, slug: `t-${id8()}` }).returning({ id: teams.id })
  await tx.insert(teamMembers).values(people.map((userId, n) => ({ teamId: team!.id, userId, role: n === 0 ? (`owner` as const) : (`member` as const) })))
  const [other] = await tx.insert(teams).values({ name: `O`, slug: `o-${id8()}` }).returning({ id: teams.id })
  const endpoints = new Map<string, string>()
  const box = async (userId: string, name: string, opts: { caps?: string[]; install?: boolean; shared?: string[] } = {}) => {
    const [row] = await tx
      .insert(devices)
      .values({ userId, deviceId: `${name}-${id8()}`, label: name, kind: `server`, caps: opts.caps ?? [FILE_TRANSFER_CAP], lastSeenAt: new Date(), sharedTeamIds: opts.shared ?? [] })
      .returning({ id: devices.id, deviceId: devices.deviceId })
    if (opts.install !== false) {
      const endpointId = `${name}-ep-${id8()}`
      await tx.insert(installs).values({ userId, kind: `daemon`, deviceId: row!.id, endpointId, x25519Public: Buffer.alloc(32, name.length).toString(`base64url`) })
      endpoints.set(row!.deviceId, endpointId)
    }
    return row!.deviceId
  }
  const mint = await box(owner, `mint`)
  const deck = await box(owner, `steamdeck`)
  const old = await box(owner, `old`, { caps: [] })
  const bare = await box(owner, `bare`, { install: false })
  const theirs = await box(mate, `theirs`)
  const otherBox = await box(owner, `other-box`, { shared: [other!.id] })
  const sessionId = crypto.randomUUID()
  await tx.insert(sessions).values({ id: sessionId, rootId: sessionId, teamId: team!.id, userId: owner, deviceId: mint, status: `running`, shared: false, title: `t` } as never)
  const callerOf = (userId: string) =>
    devicesRouter.createCaller({
      session: { user: { id: userId }, session: { id: `s` } },
      db: tx,
      request: new Request(`http://x.test`),
    } as never)
  return { owner, mate, mint, deck, old, bare, theirs, otherBox, teamId: team!.id, sessionId, endpoints, callerOf }
}

const send = { srcPath: `~/WoW/Interface/AddOns`, destPath: `~/Games/WoW/Interface/AddOns` }

describe.skipIf(!ready.ok)(`device transfers (#FEED-295)`, () => {
  it(`runs only between the caller's own capable machines and queues one command per side`, async () => {
    await inRollback(async (tx) => {
      const w = await world(tx)
      const me = w.callerOf(w.owner)
      // Another person's device is refused, either side, with the same answer as an unknown one.
      await expect(me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.theirs, ...send })).rejects.toThrow(TRANSFER_COPY.notYours)
      await expect(me.startTransfer({ fromDeviceId: w.theirs, toDeviceId: w.mint, ...send })).rejects.toThrow(TRANSFER_COPY.notYours)
      await expect(me.startTransfer({ fromDeviceId: w.mint, toDeviceId: `nope`, ...send })).rejects.toThrow(TRANSFER_COPY.notYours)
      // The teammate cannot use the owner's machines either.
      await expect(w.callerOf(w.mate).startTransfer({ fromDeviceId: w.mint, toDeviceId: w.deck, ...send })).rejects.toThrow(TRANSFER_COPY.notYours)
      await expect(me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.mint, ...send })).rejects.toThrow(TRANSFER_COPY.sameDevice)
      await expect(me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.old, ...send })).rejects.toThrow(/without file transfers/)
      await expect(me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.bare, ...send })).rejects.toThrow(/no P2P node/)
      await expect(me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.deck, srcPath: `rel/path`, destPath: `~/x` })).rejects.toThrow(/srcPath/)
      // A team session cannot reach a machine shared only with another team.
      await expect(me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.otherBox, ...send, teamId: w.teamId })).rejects.toThrow(/another team/)
      expect(await tx.select().from(deviceTransfers).where(eq(deviceTransfers.userId, w.owner))).toHaveLength(0)

      const started = await me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.deck, ...send, sessionId: w.sessionId, teamId: w.teamId })
      expect(started).toMatchObject({ status: `queued`, fromDeviceId: w.mint, toDeviceId: w.deck, fromOnline: true, toOnline: true })
      const commands = await tx
        .select({ kind: deviceCommands.kind, payload: deviceCommands.payload, status: deviceCommands.status })
        .from(deviceCommands)
        .where(sql`${deviceCommands.payload}->>'transferId' = ${started.transferId}`)
      expect(commands.sort((a, b) => a.kind.localeCompare(b.kind))).toEqual([
        { kind: `file_receive`, status: `pending`, payload: { transferId: started.transferId, destPath: send.destPath, peerDeviceId: w.mint, peers: [w.endpoints.get(w.mint)] } },
        { kind: `file_send`, status: `pending`, payload: { transferId: started.transferId, srcPath: send.srcPath, peerDeviceId: w.deck, peers: [w.endpoints.get(w.deck)] } },
      ])
      // Another person can neither read, report on nor cancel it.
      const them = w.callerOf(w.mate)
      await expect(them.transfer({ transferId: started.transferId })).rejects.toThrow(TRANSFER_COPY.notFound)
      await expect(them.cancelTransfer({ transferId: started.transferId })).rejects.toThrow(TRANSFER_COPY.notFound)
      await expect(them.reportTransfer({ transferId: started.transferId, role: `receive`, status: `done` })).rejects.toThrow(TRANSFER_COPY.notFound)
      expect(await them.transfers({ limit: 20 })).toEqual([])
    })
  })

  it(`tracks the destination's progress; only its done ends it, closing the commands and waking the session and its task`, async () => {
    await inRollback(async (tx) => {
      const w = await world(tx)
      const me = w.callerOf(w.owner)
      const started = await me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.deck, ...send, sessionId: w.sessionId, teamId: w.teamId })
      const taskId = `tsk_${id8()}`
      const now = new Date()
      await tx.insert(mcpTasks).values({ id: taskId, userId: w.owner, grant: `member`, principal: `user:${w.owner}|client:c`, teamId: w.teamId, kind: `transfer`, ref: started.transferId, createdAt: now, lastUpdatedAt: now, expiresAt: new Date(now.getTime() + 60_000) } as never)

      expect(await me.reportTransfer({ transferId: started.transferId, role: `receive`, status: `running`, files: 812, filesDone: 10, bytes: 5_900_000_000, bytesDone: 100_000_000 })).toEqual({ ok: true, status: `running` })
      // The source's counters never overwrite the destination's, and its `done` is not the end.
      expect(await me.reportTransfer({ transferId: started.transferId, role: `send`, status: `done`, bytesDone: 1 })).toEqual({ ok: true, status: `running` })
      expect(await me.transfer({ transferId: started.transferId })).toMatchObject({ status: `running`, files: 812, filesDone: 10, bytes: 5_900_000_000, bytesDone: 100_000_000 })

      expect(await me.reportTransfer({ transferId: started.transferId, role: `receive`, status: `done`, files: 812, filesDone: 812, bytes: 5_900_000_000, bytesDone: 5_900_000_000, skipped: [`link`] })).toEqual({ ok: true, status: `done` })
      const pending = await tx.select().from(deviceCommands).where(and(sql`${deviceCommands.payload}->>'transferId' = ${started.transferId}`, eq(deviceCommands.status, `pending`)))
      expect(pending).toEqual([])
      const wake = await tx.select({ body: messages.body }).from(messages).where(inSession(w.sessionId))
      expect(wake.map((m) => m.body)).toEqual([expect.stringMatching(/^Transfer .* done: mint:~\/WoW\/Interface\/AddOns → steamdeck:.*812 files, 5\.9 GB, every checksum verified, 1 skipped/)])
      const [task] = await tx.select().from(mcpTasks).where(eq(mcpTasks.id, taskId))
      expect(task).toMatchObject({ status: `completed` })
      // Terminal once: a late report changes nothing and stops the machine.
      expect(await me.reportTransfer({ transferId: started.transferId, role: `receive`, status: `failed`, error: `late` })).toEqual({ ok: false, status: `done` })
      expect(await me.cancelTransfer({ transferId: started.transferId })).toMatchObject({ status: `done` })
    })
  })

  it(`cancel ends it for both machines; a failure carries its reason`, async () => {
    await inRollback(async (tx) => {
      const w = await world(tx)
      const me = w.callerOf(w.owner)
      const a = await me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.deck, ...send })
      expect(await me.cancelTransfer({ transferId: a.transferId })).toMatchObject({ status: `cancelled` })
      expect(await me.reportTransfer({ transferId: a.transferId, role: `receive`, status: `running` })).toEqual({ ok: false, status: `cancelled` })
      const failed = await tx.select({ status: deviceCommands.status }).from(deviceCommands).where(sql`${deviceCommands.payload}->>'transferId' = ${a.transferId}`)
      expect(failed.map((c) => c.status)).toEqual([`failed`, `failed`])

      const b = await me.startTransfer({ fromDeviceId: w.deck, toDeviceId: w.mint, ...send })
      await me.reportTransfer({ transferId: b.transferId, role: `send`, status: `failed`, error: `source: ~/WoW is protected` })
      expect(await me.transfer({ transferId: b.transferId })).toMatchObject({ status: `failed`, error: `source: ~/WoW is protected` })
      expect((await me.transfers({ limit: 20 })).map((t) => t.transferId)).toEqual([b.transferId, a.transferId])
    })
  })

  it(`allows at most ${TRANSFERS_LIVE_MAX} live transfers per person`, async () => {
    await inRollback(async (tx) => {
      const w = await world(tx)
      const me = w.callerOf(w.owner)
      for (let n = 0; n < TRANSFERS_LIVE_MAX; n++) await me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.deck, srcPath: `~/a${n}`, destPath: `~/b${n}` })
      await expect(me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.deck, ...send })).rejects.toThrow(TRANSFER_COPY.tooMany)
      const [first] = await me.transfers({ limit: 1 })
      await me.cancelTransfer({ transferId: first!.transferId })
      await expect(me.startTransfer({ fromDeviceId: w.mint, toDeviceId: w.deck, ...send })).resolves.toMatchObject({ status: `queued` })
    })
  })
})
