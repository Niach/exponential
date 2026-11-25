// @vitest-environment node
// EXP-2110 against the REAL migrated Postgres: a call's durable record from
// the mint to the relay's end report (stored twice = once), the closes without
// a report, the host's report, the reads' authorization, retention, and the
// account deletion's cascade.
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest"
import { eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, devices, sessions, steerRelayBoots, teamMembers, teams, voiceCalls } from "@/db/schema"
import type { RelayCallReports } from "@/lib/steer"
import { probeRealDb } from "@/test/real-db"

vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => ({ url: `https://relay.example`, secret: `relay-secret` }),
  relayPostNudge: async () => ({ delivered: true }),
}))

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_voice_calls_open') AS ok`)
if (!ready.ok) console.info(`[voice-call-records.real-db] skipped: ${ready.reason}`)

const records = await import(`./voice-call-records`)
const { voiceRouter } = await import(`@/lib/trpc/voice`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const boot = `test-${id8()}`
const made = { teams: [] as string[], users: [] as string[], boots: [boot] }
const caller = (userId: string) =>
  voiceRouter.createCaller({
    session: { user: { id: userId } },
    db,
    request: new Request(`http://x.test`, { headers: { "x-client-version": `android/0.14.159` } }),
  } as never)

async function user(): Promise<string> {
  const id = `calls-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@calls.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

interface World {
  owner: string
  member: string
  teamId: string
  agentId: string
  deviceId: string
}

async function world(): Promise<World> {
  const [owner, member] = [await user(), await user()]
  const [row] = await db.insert(teams).values({ name: `Calls`, slug: `calls-${id8()}` }).returning({ id: teams.id })
  const teamId = row!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: member, role: `member` },
  ])
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({
    userId: member,
    deviceId,
    label: deviceId,
    kind: `desktop`,
    lastSeenAt: new Date(),
    doctor: { checkedAt: new Date().toISOString(), items: [{ key: `speech`, group: `voice`, state: `ok` }] } as never,
  })
  const [agent] = await db
    .insert(agents)
    .values({ teamId, name: `Agent`, icon: null, color: null, instructions: ``, deviceId, ownerId: member, role: `agent` })
    .returning({ id: agents.id })
  return { owner, member, teamId, agentId: agent!.id, deviceId }
}

const mint = async (w: World) => (await caller(w.member).startCall({ agentId: w.agentId, platform: `phone` })).callId
const read = async (callId: string) => (await db.select().from(voiceCalls).where(eq(voiceCalls.callId, callId)))[0]!
const age = (callId: string, ms: number) =>
  db.update(voiceCalls).set({ mintedAt: new Date(Date.now() - ms) }).where(eq(voiceCalls.callId, callId))

function answerOf(over: Partial<RelayCallReports> = {}): RelayCallReports {
  return { boot, startedAt: Date.now() - 3_600_000, now: Date.now(), ended: [], more: false, live: [], counters: { callFramesDropped: 3 }, ...over }
}
function endOf(callId: string, over: Partial<RelayCallReports[`ended`][number]> = {}): RelayCallReports[`ended`][number] {
  const t = Date.now()
  return {
    seq: 1, callId, rangAt: t - 9_000, pairedAt: t - 8_900, answeredAt: null, firstDownAt: t - 5_000, lastUpAt: t - 100, lastDownAt: t - 400,
    framesUp: 400, framesDown: 120, framesDropped: 2, framesSkipped: 1, endedAt: t, by: `relay`, reason: `idle`, ...over,
  }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  await db.delete(steerRelayBoots).where(inArray(steerRelayBoots.boot, made.boots))
})

describe.skipIf(!ready.ok)(`EXP-2110 the durable call record`, () => {
  let w: World
  beforeAll(async () => {
    w = await world()
  })

  it(`the mint opens it, the host's ticket joins, the relay's end closes it; storing it twice changes nothing`, async () => {
    const callId = await mint(w)
    expect(await read(callId)).toMatchObject({
      tag: callId.slice(0, 8), userId: w.member, teamId: w.teamId, agentId: w.agentId, deviceId: w.deviceId,
      platform: `android`, appVersion: `0.14.159`, mode: `call`, sealed: false, endedAt: null, reason: null,
    })
    await caller(w.member).callHostTicket({ callId, deviceId: w.deviceId, agentId: w.agentId })
    const reports = answerOf({ ended: [endOf(callId)] })
    expect(await records.storeCallReports(reports)).toBe(1)
    const row = await read(callId)
    expect(row).toMatchObject({ endedBy: `relay`, reason: `idle`, framesUp: 400, framesDown: 120, framesDropped: 2, framesSkipped: 1, relayBoot: boot, answeredMs: null })
    expect(row.hostTicketAt).not.toBeNull()
    expect(row.rangAt?.getTime()).toBe(reports.ended[0]!.rangAt)
    await records.storeCallReports(reports)
    expect(await read(callId)).toEqual(row)
    // A late refusal of a replayed ticket never overwrites the call.
    await records.storeCallReports(answerOf({ ended: [endOf(callId, { seq: 2, rangAt: null, reason: `busy` })] }))
    expect(await read(callId)).toEqual(row)
  })

  it(`the host's report of the hang-up merges with the relay's, in either order`, async () => {
    const first = await mint(w)
    const second = await mint(w)
    for (const [callId, hostFirst] of [[first, true], [second, false]] as const) {
      const sid = crypto.randomUUID()
      const [session] = await db
        .insert(sessions)
        .values({ id: sid, rootId: sid, teamId: w.teamId, userId: w.member, deviceId: w.deviceId, status: `ended`, voiceCall: { callId, startedAt: ``, segmentStartedAt: ``, endedAt: null, durationMs: 0, summaryPending: false, readUpTo: `` } } as never)
        .returning({ id: sessions.id })
      const host = () => caller(w.member).callEnded({ sessionId: session!.id, report: { by: `relay`, reason: `idle`, answeredMs: 4_200 } })
      const relay = () => records.storeCallReports(answerOf({ ended: [endOf(callId, { answeredAt: Date.now() })] }))
      for (const run of hostFirst ? [host, relay] : [relay, host]) await run()
      expect(await read(callId)).toMatchObject({ hostEndedBy: `relay`, hostReason: `idle`, answeredMs: 4_200, endedBy: `relay`, reason: `idle`, framesUp: 400 })
    }
  })

  it(`a call never paired, a refused one and one never dialled all end in a record`, async () => {
    const noAnswer = await mint(w)
    const offline = await mint(w)
    const undialled = await mint(w)
    await records.storeCallReports(
      answerOf({
        ended: [
          endOf(noAnswer, { pairedAt: null, firstDownAt: null, reason: `no_answer` }),
          endOf(offline, { seq: 2, rangAt: null, pairedAt: null, reason: `device_offline` }),
        ],
      })
    )
    await age(undialled, 10 * 60_000)
    await records.closeLostCallRecords(answerOf())
    expect(await read(noAnswer)).toMatchObject({ endedBy: `relay`, reason: `no_answer`, pairedAt: null })
    expect(await read(offline)).toMatchObject({ endedBy: `relay`, reason: `device_offline`, rangAt: null })
    expect(await read(undialled)).toMatchObject({ endedBy: `server`, reason: `not_dialled` })
  })

  it(`a relay restart closes the live call's record, and never a call that runs on the new process`, async () => {
    const lost = await mint(w)
    const runs = await mint(w)
    const live = { callId: lost, rangAt: Date.now() - 600_000, pairedAt: Date.now() - 599_000, answeredAt: null }
    await records.storeCallReports(answerOf({ live: [live] }))
    expect(await read(lost)).toMatchObject({ relayBoot: boot, endedAt: null })
    for (const callId of [lost, runs]) await age(callId, 10 * 60_000)
    const next = `${boot}-2`
    made.boots.push(next)
    const restartedAt = Date.now() - 120_000
    const after = answerOf({ boot: next, startedAt: restartedAt, live: [{ ...live, callId: runs }] })
    await records.storeCallReports(after)
    await records.closeLostCallRecords(after)
    expect(await read(lost)).toMatchObject({ endedBy: `server`, reason: `relay_restarted` })
    expect((await read(lost)).endedAt?.getTime()).toBe(restartedAt)
    expect(await read(runs)).toMatchObject({ endedAt: null, relayBoot: next })
    // The relay's own report, when it comes, replaces what the sweep wrote.
    await records.storeCallReports(answerOf({ boot: next, ended: [endOf(runs)] }))
    expect(await read(runs)).toMatchObject({ endedBy: `relay`, reason: `idle` })
    // The counters of both processes add up.
    const rows = await db.select().from(steerRelayBoots).where(inArray(steerRelayBoots.boot, [boot, next]))
    expect(rows.map((row) => row.counters.callFramesDropped)).toEqual([3, 3])
    expect((await records.relayCounterTotals()).counters.callFramesDropped).toBeGreaterThanOrEqual(6)
  })

  it(`collecting acknowledges what it stored, and an unreachable relay closes nothing young`, async () => {
    const callId = await mint(w)
    await age(callId, 10 * 60_000)
    const config = { url: `https://relay.example`, secret: `s` }
    await records.collectCallReports(() => new Date(), config, async () => ({ ok: false, status: 404, json: async () => ({}) }))
    expect((await read(callId)).endedAt).toBeNull()
    const bodies: unknown[] = []
    const result = await records.collectCallReports(() => new Date(), config, async (_url, init) => {
      const body = JSON.parse(init?.body ?? `{}`) as { ack?: { seq: number } }
      bodies.push(body)
      return { ok: true, status: 200, json: async () => answerOf({ ended: body.ack ? [] : [endOf(callId, { seq: 7 })] }) }
    })
    expect(result.stored).toBe(1)
    expect(bodies).toEqual([{}, { ack: { boot, seq: 7 } }])
    expect(await read(callId)).toMatchObject({ endedBy: `relay`, reason: `idle` })
  })

  it(`the team's owners read it, a member does not; support reads by tag`, async () => {
    const callId = await mint(w)
    expect((await caller(w.owner).callRecords({ teamId: w.teamId })).map((row) => row.callId)).toContain(callId)
    await expect(caller(w.member).callRecords({ teamId: w.teamId })).rejects.toMatchObject({ code: `FORBIDDEN` })
    const other = await world()
    await expect(caller(other.owner).callRecords({ teamId: w.teamId })).rejects.toThrow()
    await expect(caller(w.owner).callRecordsByTag({ tag: callId.slice(0, 8) })).rejects.toThrow()
    await db.execute(sql`UPDATE users SET is_admin = true WHERE id = ${w.owner}`)
    expect((await caller(w.owner).callRecordsByTag({ tag: callId.slice(0, 8) })).map((row) => row.callId)).toContain(callId)
  })

  it(`the record stays out when the mint's write fails: the call goes on`, async () => {
    const broken = { insert: () => { throw new Error(`no database`) } } as never
    const error = vi.spyOn(console, `error`).mockImplementation(() => {})
    await records.recordCallMinted(
      { callId: crypto.randomUUID(), userId: w.member, teamId: w.teamId, agentId: w.agentId, deviceId: w.deviceId, platform: `web`, mode: `call`, sealed: false },
      new Request(`http://x.test`), new Date(), broken
    )
    expect(error).toHaveBeenCalled()
    error.mockRestore()
  })

  it(`retention deletes past 30 days; deleting the account deletes its records`, async () => {
    const old = await mint(w)
    const fresh = await mint(w)
    await age(old, 31 * 24 * 60 * 60_000)
    await records.runCallRecordRetention()
    expect(await read(old)).toBeUndefined()
    expect(await read(fresh)).toBeDefined()
    const gone = await world()
    const theirs = await mint(gone)
    expect(await read(theirs)).toBeDefined()
    await db.execute(sql`DELETE FROM users WHERE id = ${gone.member}`)
    expect(await read(theirs)).toBeUndefined()
  })
})
