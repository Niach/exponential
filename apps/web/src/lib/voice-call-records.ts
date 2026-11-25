// EXP-2110 (etalk voice-call.json `record`): ONE durable record per streamed
// voice call, `voice_calls`. `voice.startCall` opens it (the mint), the host's
// ticket and its hang-up report add what the host knows, and the relay's end
// report closes it. The relay has no database and no address of this server,
// so the server COLLECTS: once a minute (the watchdog) and right after a
// host's hang-up report, over the authenticated relay route every other admin
// call uses. A report stays on the relay until it is acknowledged, and is
// stored before it is, so collecting twice (a lost answer, two processes)
// writes the same row. A call no report will ever come for is closed here with
// a reason (`callRecordClose`). Metadata only: never a word, a name, a frame.
// Every write is best effort: a record never fails or delays a call.

import { and, desc, eq, isNull, lt, lte, sql } from "drizzle-orm"
import { VOICE_CALL_RECORD, callRecordClose } from "@exp/etalk/voice-call"

import { db } from "@/db/connection"
import { sessions, steerRelayBoots, voiceCalls } from "@/db/schema"
import { CLIENT_VERSION_HEADER, parseClientVersionHeader } from "@/lib/client-version"
import { getSteerRelayConfig, relayPostCallReports, type RelayCallReports, type SteerRelayConfig } from "@/lib/steer"
import { callLogTag } from "@/lib/voice-call-id"

type Executor = Pick<typeof db, `select` | `insert` | `update` | `delete`>

export const CALL_RECORD_RETENTION_MS = VOICE_CALL_RECORD.retentionDays * 24 * 60 * 60 * 1000

const bestEffort = async (what: string, callId: string | null, run: () => Promise<unknown>): Promise<void> => {
  try {
    await run()
  } catch (err) {
    console.error(`[voice] call ${callLogTag(callId)}: ${what} was not recorded:`, err)
  }
}

// ── What the server knows ────────────────────────────────────────────────────

export interface MintedCall {
  callId: string
  userId: string
  teamId: string
  agentId: string
  deviceId: string
  platform: string
  mode: `call` | `turn`
  sealed: boolean
}

/** The mint (`voice.startCall`). */
export async function recordCallMinted(call: MintedCall, request: Request, now: Date = new Date(), executor: Executor = db): Promise<void> {
  await bestEffort(`the mint`, call.callId, () =>
    executor
      .insert(voiceCalls)
      .values({
        ...call,
        tag: callLogTag(call.callId),
        appVersion: parseClientVersionHeader(request.headers.get(CLIENT_VERSION_HEADER))?.version.slice(0, 64) ?? null,
        mintedAt: now,
      })
      .onConflictDoNothing()
  )
}

/** The host asked for its ticket (the first time counts). */
export async function recordCallHostTicket(callId: string, userId: string, now: Date = new Date(), executor: Executor = db): Promise<void> {
  await bestEffort(`the host ticket`, callId, () =>
    executor
      .update(voiceCalls)
      .set({ hostTicketAt: now })
      .where(and(eq(voiceCalls.callId, callId), eq(voiceCalls.userId, userId), isNull(voiceCalls.hostTicketAt)))
  )
}

export interface HostCallReport {
  by: string
  reason: string
  /** From the ring's start on the host's own clock; absent = never answered. */
  answeredMs?: number
}

/** The host's own report of the hang-up (`voice.callEnded`), found through
 *  its call session. It never closes a record: the relay's report does. */
export async function recordCallHostEnd(args: { userId: string; sessionId: string; report?: HostCallReport }, executor: Executor = db): Promise<void> {
  const { report } = args
  if (!report) return
  await bestEffort(`the host's report`, null, async () => {
    const [row] = await executor
      .select({ voiceCall: sessions.voiceCall })
      .from(sessions)
      .where(and(eq(sessions.id, args.sessionId), eq(sessions.userId, args.userId)))
      .limit(1)
    const callId = row?.voiceCall?.callId
    if (!callId || callLogTag(callId) === `local`) return
    await executor
      .update(voiceCalls)
      .set({
        hostEndedBy: report.by,
        hostReason: report.reason,
        ...(report.answeredMs === undefined ? {} : { answeredMs: report.answeredMs }),
      })
      .where(and(eq(voiceCalls.callId, callId), eq(voiceCalls.userId, args.userId)))
  })
}

// ── What the relay knows ─────────────────────────────────────────────────────

const at = (ms: number | null | undefined): Date | null => (typeof ms === `number` && Number.isFinite(ms) ? new Date(ms) : null)
const count = (n: unknown): number => (typeof n === `number` && Number.isFinite(n) ? Math.min(Math.max(0, Math.round(n)), 2_147_483_647) : 0)
const code = (text: unknown, max: number): string => (typeof text === `string` ? text.slice(0, max) : `other`)
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** The relay's own answer instant (a clear call), unless the host said it. */
const answeredMsOf = (rangAt: number | null, answeredAt: number | null) =>
  rangAt !== null && answeredAt !== null ? sql`coalesce(${voiceCalls.answeredMs}, ${count(answeredAt - rangAt)}::int)` : voiceCalls.answeredMs

/** Stores one answer of the relay. Idempotent. A refused ring never replaces
 *  what a ring of the same call reported; anything else does (a record the
 *  sweep closed, too). Answers how many records it wrote. */
export async function storeCallReports(reports: RelayCallReports, executor: Executor = db): Promise<number> {
  let stored = 0
  for (const end of reports.ended) {
    if (!UUID.test(end.callId)) continue
    const refused = end.rangAt === null
    const rows = await executor
      .update(voiceCalls)
      .set({
        relayBoot: reports.boot.slice(0, 64),
        ...(refused
          ? {}
          : {
              rangAt: at(end.rangAt),
              pairedAt: at(end.pairedAt),
              answeredMs: answeredMsOf(end.rangAt, end.answeredAt),
              firstDownAt: at(end.firstDownAt),
              lastUpAt: at(end.lastUpAt),
              lastDownAt: at(end.lastDownAt),
              framesUp: count(end.framesUp),
              framesDown: count(end.framesDown),
              framesDropped: count(end.framesDropped),
              framesSkipped: count(end.framesSkipped),
            }),
        endedAt: at(end.endedAt),
        endedBy: code(end.by, 16),
        reason: code(end.reason, 64),
      })
      .where(and(eq(voiceCalls.callId, end.callId), refused ? isNull(voiceCalls.rangAt) : undefined))
      .returning({ callId: voiceCalls.callId })
    stored += rows.length
  }
  for (const live of reports.live) {
    if (!UUID.test(live.callId)) continue
    await executor
      .update(voiceCalls)
      .set({
        relayBoot: reports.boot.slice(0, 64),
        rangAt: at(live.rangAt),
        pairedAt: at(live.pairedAt),
        answeredMs: answeredMsOf(live.rangAt, live.answeredAt),
      })
      .where(and(eq(voiceCalls.callId, live.callId), isNull(voiceCalls.endedAt)))
  }
  const counters = Object.fromEntries(Object.entries(reports.counters).filter((entry): entry is [string, number] => typeof entry[1] === `number`))
  const boot = { startedAt: new Date(reports.startedAt), seenAt: new Date(reports.now), counters }
  await executor
    .insert(steerRelayBoots)
    .values({ boot: reports.boot.slice(0, 64), ...boot })
    .onConflictDoUpdate({ target: steerRelayBoots.boot, set: boot, setWhere: lte(steerRelayBoots.seenAt, boot.seenAt) })
  return stored
}

/** Closes every open record no report will come for (`callRecordClose`).
 *  `reports` = the relay's answer AFTER its ended calls were stored; null =
 *  it did not answer. */
export async function closeLostCallRecords(
  reports: Pick<RelayCallReports, `boot` | `startedAt` | `live`> | null,
  now: Date = new Date(),
  executor: Executor = db
): Promise<number> {
  const open = await executor
    .select({ callId: voiceCalls.callId, mintedAt: voiceCalls.mintedAt, rangAt: voiceCalls.rangAt, relayBoot: voiceCalls.relayBoot })
    .from(voiceCalls)
    .where(and(isNull(voiceCalls.endedAt), lt(voiceCalls.mintedAt, new Date(now.getTime() - VOICE_CALL_RECORD.closeAfterMs))))
    .limit(500)
  const live = new Set(reports?.live.map((call) => call.callId))
  let closed = 0
  for (const row of open) {
    const reason = callRecordClose({
      ageMs: now.getTime() - row.mintedAt.getTime(),
      rang: row.rangAt !== null,
      relay: reports === null ? null : { live: live.has(row.callId), startedAfterMint: reports.startedAt > row.mintedAt.getTime(), sameBoot: row.relayBoot === reports.boot },
    })
    if (!reason) continue
    const rows = await executor
      .update(voiceCalls)
      .set({ endedAt: reason === `relay_restarted` && reports ? new Date(reports.startedAt) : now, endedBy: `server`, reason })
      .where(and(eq(voiceCalls.callId, row.callId), isNull(voiceCalls.endedAt)))
      .returning({ callId: voiceCalls.callId })
    closed += rows.length
    if (rows.length > 0) console.info(`[voice] call ${callLogTag(row.callId)}: its record is closed without an end report: ${reason}`)
  }
  return closed
}

const MAX_PAGES = 20

/** One collection: every report stored, acknowledged, then the records no
 *  report will come for closed. */
export async function collectCallReports(
  now: () => Date = () => new Date(),
  config: SteerRelayConfig | null = getSteerRelayConfig(),
  fetchImpl?: Parameters<typeof relayPostCallReports>[2]
): Promise<{ stored: number; closed: number }> {
  if (!config) return { stored: 0, closed: 0 }
  let stored = 0
  let ack: { boot: string; seq: number } | null = null
  let last: RelayCallReports | null = null
  for (let page = 0; page < MAX_PAGES; page++) {
    const reports: RelayCallReports | null = await relayPostCallReports(config, ack, fetchImpl)
    last = reports
    if (!reports) break
    stored += await storeCallReports(reports)
    const seq = reports.ended.at(-1)?.seq
    if (seq === undefined) break
    ack = { boot: reports.boot, seq }
  }
  // Ended calls still on the relay (more than MAX_PAGES): nothing is lost yet.
  if (last && last.ended.length > 0) return { stored, closed: 0 }
  return { stored, closed: await closeLostCallRecords(last, now()) }
}

/** After a host's hang-up report: the relay's end is there by now. */
export function collectCallReportsSoon(): void {
  setTimeout(() => void collectCallReports().catch((err) => console.error(`[voice] call reports failed`, err)), 2_000).unref?.()
}

// ── Retention ────────────────────────────────────────────────────────────────

/** Every record minted, and every relay process last seen, before the window. */
export async function runCallRecordRetention(now: Date = new Date(), executor: Executor = db): Promise<{ callRecordsDeleted: number }> {
  const cutoff = new Date(now.getTime() - CALL_RECORD_RETENTION_MS)
  const deleted = await executor.delete(voiceCalls).where(lte(voiceCalls.mintedAt, cutoff)).returning({ callId: voiceCalls.callId })
  await executor.delete(steerRelayBoots).where(lte(steerRelayBoots.seenAt, cutoff))
  return { callRecordsDeleted: deleted.length }
}

// ── Reads ────────────────────────────────────────────────────────────────────

/** One team's newest records (its owners only: the router checks). */
export async function teamCallRecords(teamId: string, limit: number, executor: Executor = db) {
  return executor.select().from(voiceCalls).where(eq(voiceCalls.teamId, teamId)).orderBy(desc(voiceCalls.mintedAt)).limit(limit)
}

/** Support: the calls behind a tag (or a whole call id) out of anyone's log. */
export async function callRecordsByTag(tag: string, executor: Executor = db) {
  return executor
    .select()
    .from(voiceCalls)
    .where(eq(voiceCalls.tag, tag.slice(0, 8).toLowerCase()))
    .orderBy(desc(voiceCalls.mintedAt))
    .limit(20)
}

/** The relay's counters over every process seen in the window, summed. */
export async function relayCounterTotals(executor: Executor = db): Promise<{ boots: number; since: string | null; counters: Record<string, number> }> {
  const rows = await executor.select().from(steerRelayBoots)
  const counters: Record<string, number> = {}
  for (const row of rows) for (const [name, value] of Object.entries(row.counters)) counters[name] = (counters[name] ?? 0) + value
  const since = rows.map((row) => row.startedAt.getTime()).sort((a, b) => a - b)[0]
  return { boots: rows.length, since: since === undefined ? null : new Date(since).toISOString(), counters }
}
