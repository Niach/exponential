import { flagQueuedChecks, sweepUnsettledChecks } from "@/lib/integrations/pr-checks"
import { and, asc, eq, inArray, isNotNull, isNull, lt, notInArray, or, sql, type SQL } from "drizzle-orm"
import type { CodingSessionBlocked } from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { stampMs } from "@exp/eagent/copy"
import { db } from "@/db/connection"
import { devices, messages, sessions, userVoiceSettings } from "@/db/schema"
import { fireScheduledTriggers } from "@/lib/agents/triggers"
import { pruneEvents } from "@/lib/events"
import { purgeOldAppRecordEvents, purgeTrashedAppRecords } from "@/lib/app-records-purge"
import { BUS_COPY, busLine } from "@/lib/sessions/bus"
import { hostRowCondition, hostedByCondition, resolveSessionHost } from "@/lib/sessions/host"
import { pendingInput, sessionConversation } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import { reportSchedulerRun } from "@/lib/metrics/registry"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"
import { autoStopHours, MEETING_AUTO_STOP } from "@exp/etalk/meeting"
import { postMeetingStopNote } from "@/lib/sessions/meeting-access"

// NEW-1: the server watchdog (every minute; contract `sessionWatchdog`).
// Nothing waits on a device's good behaviour: what stalls is flagged on
// the row (`blocked.kind`), lands under Needs you (`needs_input`) and is
// said ONCE in the parent's conversation through the bus. Each flag has
// exactly one clearer: a claim (`unclaimed`; #FEED-235: on a started session
// also a consume or a busy report), the next device report
// (`stuck`), the device's next heartbeat (`device_offline`). FEED-129: the
// device-written usage wall (`rate_limit`) is the device's to clear, or
// this watchdog's once its `resetsAt` passed. EXP-1374: an INTERRUPTION
// (`device_offline`, `restarted`) is never on a person: a mid-turn run
// (`resume`) is continued by its device when back (its report clears it);
// only one the device left alone `resumeMinutes` after it came back is, or
// (#EXP-1675) one whose device is still away `offlineMinutes` after the flag.
// EXP-1536: a failed start (`start_failed`) is device-written like a wall
// but never said as one; the device's next report (the relaunch) clears it.

export const WATCHDOG_INTERVAL_MS = 60 * 1000

const MIN = 60 * 1000
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

export interface WatchdogRun {
  unclaimed: number
  nudged: number
  /** #FEED-235: input no turn ran for, on a person. */
  undelivered: number
  stuck: number
  offline: number
  /** #FEED-211: parked runs that spoke again, busy again. */
  spoke: number
  /** EXP-1374: interruptions their device never continued (Needs you). */
  unresumed: number
  /** #EXP-1675: cut runs whose device stayed away (Needs you). */
  offlineLong: number
  /** FEED-129: usage walls whose reset time passed, cleared this tick. */
  expired: number
  /** NEW-2: agent schedule firings this tick, and pruned event rows. */
  fired: number
  pruned: number
  /** NEW-57: expired meeting transcript batches deleted this tick. */
  meetingPruned: number
  /** NEW-57: meetings stopped by `autoStop` this tick. */
  meetingsStopped: number
  /** VAPP-159: trashed app records hard-deleted this tick. */
  recordsPurged: number
  recordEventsPurged: number
  /** eApps R8: records the publish sweeps moved. */
  recordsMigrated: number
  /** #VAPP-220: app automation schedules run this tick. */
  automations: number
  hookNoncesPruned: number
  /** #VAPP-239: device-routed upstream calls past their life. */
  deviceCallsPruned: number
  /** EXP-1610: PRs whose CI check sat queued too long, flagged once. */
  checksQueued: number
  checksSwept: number
  /** #EXP-1617: our CI's jobs the runner clock moved (timeouts, lost runners, the queued flag). */
  ciJobs: number
  /** #EXP-1617: CI log chunks of deleted teams/jobs removed (hourly). */
  ciLogsPruned: number
  /** EXP-1706: live PRs whose state the reconciler re-read. */
  prsReconciled: number
  /** #EXP-1797: issues purged from the trash past its retention. */
  issuesPurged: number
  /** EXP-1709: repositories the merge train passed. */
  mergeTrainRepos: number
  commitChecksPruned: number
  /** #EXP-1616: eCode uploads never committed, swept from the bucket. */
  ecodeUploadsSwept: number
}

const LIVE = [`running`, `idle`] as const

/** #EXP-1731 (eagent subagents.json): the watchdog and the heartbeat never
 *  look at a CLI subagent row: its liveness is its parent's (a parent's turn
 *  end sweeps the strays, `lib/sessions/subagent.ts`). */
const NOT_SUBAGENT = isNull(sessions.subagentKey)

/** EXP-1374 / #EXP-1374 follow-up: a session its device flagged `background`
 *  (busy only because background tasks run) is exempt from the stuck rule,
 *  but not forever: the flag is written on a state edge and nothing resets
 *  it when the run dies with its tasks, so past this many minutes without
 *  a sign of life a background row is measured like any busy one. */
export const BACKGROUND_STUCK_MINUTES = 240

function flag(kind: CodingSessionBlocked[`kind`], now: Date): CodingSessionBlocked {
  return { kind, agent: null, window: null, resetsAt: null, since: now.toISOString() }
}

/** EXP-1374: the interruption kinds (session-card.json `interruptedKinds`). */
export const INTERRUPTIONS = [`device_offline`, `restarted`] as const

/** EXP-1374: an interruption flag. `resume` = the run was mid-turn, so its
 *  device continues it by itself; `resumeSince` = since when the device
 *  could (it is back): the `resumeMinutes` clock. */
export function interruption(
  kind: (typeof INTERRUPTIONS)[number],
  now: Date,
  resume: boolean,
  resumeSince: Date | null
): CodingSessionBlocked {
  return { ...flag(kind, now), resume, resumeSince: resumeSince?.toISOString() ?? null }
}

/** The flags a device report without `blocked` clears: the watchdog's
 *  `stuck` (activity = not stuck), the interruptions (EXP-1374: the device
 *  is back on it) and a failed start (EXP-1536: the relaunch reports). */
export const CLEARED_BY_REPORT = [`stuck`, ...INTERRUPTIONS, `start_failed`] as const

/** `sessions.report`'s verdict on `blocked`: the row's next value (the
 *  device's own replaces whatever it held; none clears a
 *  `CLEARED_BY_REPORT` flag, keeps the rest) and whether it is a NEW wall
 *  to say once (bus line + push): never one the device handles itself, and
 *  never a failed start (EXP-1536: its "Could not start" reply already
 *  reached the sender). `reported` = the input's `blocked` minus
 *  `handled`; `undefined` = the report said nothing about it. */
export function reportBlocked(
  current: CodingSessionBlocked | null,
  reported: CodingSessionBlocked | null | undefined,
  handled: boolean,
  /** #FEED-235: the report's `busy`: a turn started clears `unclaimed` on a
   *  claimed session (the device reports only once it holds the row). */
  busy?: boolean
): { next: CodingSessionBlocked | null; newly: boolean } {
  const next =
    reported !== undefined
      ? reported
      : (CLEARED_BY_REPORT as readonly string[]).includes(current?.kind ?? ``) ||
          (current?.kind === `unclaimed` && busy === true)
        ? null
        : current
  const newly = !!reported && !current && !handled && reported.kind !== `start_failed`
  return { next, newly }
}

/** The SQL test for "blocked by an interruption". */
const interrupted = () => sql`(${sessions.blocked} ->> 'kind') IN ('device_offline', 'restarted')`

/** EXP-1374: the session's latest sign of life (a row write, a message, the
 *  heartbeat's activity, an MCP call, a push). */
const liveness = () => sql`GREATEST(${sessions.updatedAt}, COALESCE(${sessions.lastActivityAt}, ${sessions.updatedAt}))`

/** `blocked` cleared and `needs_input` re-derived (an open card still waits).
 *  `kind` (FEED-146) clears only a flag of that kind: a usage wall the
 *  device wrote in between is never wiped by another clearer. */
export async function clearBlockedInTx(
  tx: Tx,
  sessionIds: readonly string[],
  kind?: CodingSessionBlocked[`kind`],
  /** EXP-1376: only a wall of this usage window (a lifted MODEL wall). */
  window?: string
): Promise<void> {
  if (sessionIds.length === 0) return
  await tx
    .update(sessions)
    .set({
      blocked: null,
      // Any open question/plan card still waits: the device's (`sessions.reply`)
      // or the session's own MCP one (same count as `stampAnsweredCardInTx`).
      needsInput: sql`EXISTS (SELECT 1 FROM ${messages} m WHERE m.conversation_id = ${sessionConversation(sessions.id)} AND m.answered_at IS NULL AND m.card->>'kind' IN ('question', 'plan'))`,
      updatedAt: new Date(),
    })
    .where(
      and(
        inArray(sessions.id, [...sessionIds]),
        ...(kind ? [sql`${sessions.blocked} ->> 'kind' = ${kind}`] : []),
        ...(window ? [sql`${sessions.blocked} ->> 'window' = ${window}`] : [])
      )
    )
}

/** A queued session nobody claimed in `unclaimedMinutes`: flagged once. */
async function flagUnclaimed(now: Date): Promise<number> {
  const cutoff = new Date(now.getTime() - contract.sessionWatchdog.unclaimedMinutes * MIN)
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id, parentId: sessions.parentId })
      .from(sessions)
      .where(
        and(
          eq(sessions.status, `queued`),
          isNull(sessions.startedAt),
          isNull(sessions.blocked),
          lt(sessions.createdAt, cutoff),
          NOT_SUBAGENT
        )
      )
      .for(`update`, { skipLocked: true })
    for (const row of rows) {
      await tx
        .update(sessions)
        .set({ blocked: flag(`unclaimed`, now), needsInput: true, updatedAt: now })
        .where(eq(sessions.id, row.id))
      await busLine(tx, row, `session_unclaimed`, BUS_COPY.session_unclaimed, { waking: true })
    }
    return rows.length
  })
}

/** A live session with pending input older than `nudgeMinutes`: its host
 *  device is nudged again (no row write; every tick while it lasts). */
async function nudgeStalled(now: Date): Promise<number> {
  const config = getSteerRelayConfig()
  if (!config) return 0
  const cutoff = new Date(now.getTime() - contract.sessionWatchdog.nudgeMinutes * MIN)
  const rows = await db
    .select({
      id: sessions.id,
      deviceId: sessions.deviceId,
      userId: sessions.userId,
      teamId: sessions.teamId,
    })
    .from(sessions)
    .where(
      and(
        inArray(sessions.status, [...LIVE]),
        NOT_SUBAGENT,
        // NULL-safe: an unblocked row has no kind at all.
        sql`(${sessions.blocked} ->> 'kind') IS DISTINCT FROM 'device_offline'`,
        // `pendingInput()` names the messages table itself: no alias here.
        sql`EXISTS (SELECT 1 FROM ${messages} WHERE ${messages.conversationId} = ${sessionConversation(sessions.id)} AND ${messages.consumedAt} IS NULL AND ${messages.createdAt} < ${cutoff} AND (${pendingInput()}))`
      )
    )
  const seen = new Set<string>()
  for (const row of rows) {
    const host = await resolveSessionHost(row)
    if (!host) continue
    const key = `${host}:${row.deviceId}`
    if (seen.has(key)) continue
    seen.add(key)
    void relayPostNudge(config, host, row.deviceId)
  }
  return seen.size
}

/** #FEED-235: input no turn ran for `undeliveredMinutes` on a claimed,
 *  not-busy live session whose host is ONLINE: pending input the nudges did
 *  not get in, or a first Request the claim delivered with nothing of the
 *  session's own since (every turn's close writes the agent's row) =
 *  `unclaimed` ('Not picked up') + Needs you, said once (waking). Cleared
 *  by a consume, a busy report or a claim. */
async function flagUndelivered(now: Date): Promise<number> {
  const cutoff = new Date(now.getTime() - contract.sessionWatchdog.undeliveredMinutes * MIN)
  const online = new Date(now.getTime() - contract.device.onlineWindowSeconds * 1000)
  const mine = sql`${messages.conversationId} = ${sessionConversation(sessions.id)}`
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id, parentId: sessions.parentId })
      .from(sessions)
      .where(
        and(
          inArray(sessions.status, [...LIVE]),
          NOT_SUBAGENT,
          eq(sessions.busy, false),
          isNull(sessions.blocked),
          isNotNull(sessions.startedAt),
          sql`EXISTS (SELECT 1 FROM ${devices} h WHERE ${hostRowCondition(`h`)} AND h.last_seen_at >= ${online})`,
          or(
            // `pendingInput()` names the messages table itself: no alias here.
            sql`EXISTS (SELECT 1 FROM ${messages} WHERE ${mine} AND ${messages.consumedAt} IS NULL AND ${messages.createdAt} < ${cutoff} AND (${pendingInput()}))`,
            and(
              lt(sessions.startedAt, cutoff),
              sql`EXISTS (SELECT 1 FROM ${messages} WHERE ${mine} AND ${messages.card} IS NULL AND (${pendingInput()}))`,
              sql`NOT EXISTS (SELECT 1 FROM ${messages} WHERE ${mine} AND ${messages.authorKind} = 'agent' AND (${messages.authorSessionId} IS NULL OR ${messages.authorSessionId} = ${sessions.id}))`
            )
          )
        )
      )
      .for(`update`, { skipLocked: true })
    for (const row of rows) {
      await tx
        .update(sessions)
        .set({ blocked: flag(`unclaimed`, now), needsInput: true, updatedAt: now })
        .where(eq(sessions.id, row.id))
      await busLine(tx, row, `session_unclaimed`, BUS_COPY.session_undelivered, { waking: true })
    }
    return rows.length
  })
}

/** A busy session with NO SIGN OF LIFE for `stuckMinutes`: flagged once.
 *  FEED-145: a device only reports on a state edge, so a long turn (a
 *  subagent fan-out, a build) is report-free by design. A session its
 *  ONLINE host lists in the heartbeat's `liveSessionIds` has a live process
 *  whose own stall watchdog (FEED-25) guards the turn: never flagged here
 *  (it read "needs input" while busy). EXP-1374 (FEED-141): an older device
 *  that never claims is measured from its LAST SIGN OF LIFE (`liveness`:
 *  the heartbeat's tool-call activity, an MCP call, a push), never from its
 *  last row write alone, and a session with background tasks is alive. The
 *  line says how long and asks to check, never to stop it. */
async function flagStuck(now: Date): Promise<number> {
  const cutoff = new Date(now.getTime() - contract.sessionWatchdog.stuckMinutes * MIN)
  const backgroundCutoff = new Date(now.getTime() - BACKGROUND_STUCK_MINUTES * MIN)
  const online = new Date(now.getTime() - contract.device.onlineWindowSeconds * 1000)
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id, parentId: sessions.parentId, alive: sql<number>`EXTRACT(EPOCH FROM ${liveness()}) * 1000`.mapWith(Number) })
      .from(sessions)
      .where(
        and(
          inArray(sessions.status, [...LIVE]),
          NOT_SUBAGENT,
          eq(sessions.busy, true),
          // A background run is alive for BACKGROUND_STUCK_MINUTES, then
          // measured like the rest.
          or(eq(sessions.background, false), sql`${liveness()} < ${backgroundCutoff}`),
          isNull(sessions.blocked),
          sql`${liveness()} < ${cutoff}`,
          // FEED-148: the resolved host's row only (`hostRowCondition`): a
          // decoy row naming the machine never vouches for a session.
          sql`NOT EXISTS (SELECT 1 FROM ${devices} h WHERE ${hostRowCondition(`h`)} AND h.last_seen_at >= ${online} AND ${sessions.id}::text = ANY(h.live_session_ids))`
        )
      )
      .for(`update`, { skipLocked: true })
    for (const row of rows) {
      await tx
        .update(sessions)
        .set({ blocked: flag(`stuck`, now), needsInput: true, updatedAt: now })
        .where(eq(sessions.id, row.id))
      await busLine(tx, row, `session_stuck`, BUS_COPY.session_stuck(silentMinutes(row.alive, now)), {
        waking: true,
      })
    }
    return rows.length
  })
}

/** Whole minutes since `alive` (epoch ms), at least the rule's own
 *  threshold (a clock skew never reads fewer). */
export function silentMinutes(alive: number, now: Date): number {
  const minutes = Number.isFinite(alive) ? Math.floor((now.getTime() - alive) / MIN) : 0
  return Math.max(contract.sessionWatchdog.stuckMinutes, minutes)
}

/** #FEED-211: a `device_offline` run (resume) that spoke to the server
 *  itself AFTER its flag, inside the online window (an MCP call, a push:
 *  `last_activity_at`), is alive: running + busy again, the flag cleared,
 *  silent, though its device's beat is still late (a stuck beat body parked
 *  healthy runs until the next beat, read as killed turns). A `restarted`
 *  run is never moved (its process died under a living device). Runs just
 *  before `flagOffline`, whose own #FEED-210 rule keeps it unflagged. */
async function restoreSpoken(now: Date): Promise<number> {
  const cutoff = new Date(now.getTime() - contract.device.onlineWindowSeconds * 1000)
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id })
      .from(sessions)
      .where(
        and(
          inArray(sessions.status, [...LIVE]),
          NOT_SUBAGENT,
          sql`${sessions.blocked} ->> 'kind' = 'device_offline'`,
          sql`(${sessions.blocked} ->> 'resume')::boolean IS TRUE`,
          sql`${sessions.lastActivityAt} > (${sessions.blocked} ->> 'since')::timestamptz`,
          sql`${sessions.lastActivityAt} BETWEEN ${cutoff} AND ${now}`
        )
      )
      .for(`update`, { skipLocked: true })
    const ids = rows.map((row) => row.id)
    if (ids.length === 0) return 0
    await tx
      .update(sessions)
      .set({ status: `running`, busy: true, background: false, updatedAt: now })
      .where(inArray(sessions.id, ids))
    await clearBlockedInTx(tx, ids, `device_offline`)
    return ids.length
  })
}

/** A BUSY session whose host device missed the online window (and that did
 *  not itself speak to the server inside it: #FEED-210, an MCP call or a
 *  push by the run proves it and its device up, whatever the beat says): idle, not
 *  busy, `device_offline` + `resume` (its next heartbeat clears it; its
 *  device continues it when back, `restoreDeviceSessionsInTx`). EXP-1374:
 *  an INTERRUPTION, not on a person (`needs_input` untouched). S-3: an idle
 *  session has nothing to interrupt and is never flagged (a `running` one
 *  between turns just reads `idle`), and the flag is silent (no bus line:
 *  an interruption is invisible ×4) until `flagOfflineTooLong`. */
async function flagOffline(now: Date): Promise<number> {
  const cutoff = new Date(now.getTime() - contract.device.onlineWindowSeconds * 1000)
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id })
      .from(sessions)
      .where(
        and(
          inArray(sessions.status, [...LIVE]),
          NOT_SUBAGENT,
          eq(sessions.busy, true),
          isNull(sessions.blocked),
          // #FEED-210: the run's own sign of life the server saw inside the
          // window (an MCP call, a push; the beat's activity is never later
          // than the beat itself) keeps it live.
          sql`NOT COALESCE(${sessions.lastActivityAt} BETWEEN ${cutoff} AND ${now}, false)`,
          // FEED-148: the resolved host's row only: a decoy row online
          // never keeps a dead host's sessions live.
          sql`NOT EXISTS (SELECT 1 FROM ${devices} h WHERE ${hostRowCondition(`h`)} AND h.last_seen_at >= ${cutoff})`
        )
      )
      .for(`update`, { skipLocked: true })
    for (const row of rows) {
      await tx
        .update(sessions)
        .set({
          status: `idle`,
          busy: false,
          background: false,
          blocked: interruption(`device_offline`, now, true, null),
          updatedAt: now,
        })
        .where(eq(sessions.id, row.id))
    }
    // A claimed session between turns (`running`, not busy) on the same dead
    // host reads `idle`: nothing to interrupt, so no flag and no bus line,
    // but older phones read "running" from `status` alone.
    await tx
      .update(sessions)
      .set({ status: `idle`, updatedAt: now })
      .where(
        and(
          eq(sessions.status, `running`),
          eq(sessions.busy, false),
          NOT_SUBAGENT,
          isNotNull(sessions.deviceId),
          sql`NOT EXISTS (SELECT 1 FROM ${devices} h WHERE ${hostRowCondition(`h`)} AND h.last_seen_at >= ${cutoff})`
        )
      )
    return rows.length
  })
}

/** EXP-1374: a mid-turn interruption its device has not continued
 *  `resumeMinutes` after it came back (an older build, a resume that
 *  failed): on a person now (Needs you), said once to its parent. The
 *  device's report or a person's message still clears it. */
async function flagUnresumed(now: Date): Promise<number> {
  const cutoff = new Date(now.getTime() - contract.sessionWatchdog.resumeMinutes * MIN)
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id, parentId: sessions.parentId })
      .from(sessions)
      .where(
        and(
          inArray(sessions.status, [...LIVE]),
          NOT_SUBAGENT,
          interrupted(),
          eq(sessions.needsInput, false),
          sql`(${sessions.blocked} ->> 'resume')::boolean IS TRUE`,
          sql`${sessions.blocked} ->> 'resumeSince' IS NOT NULL`,
          sql`${sessions.blocked} ->> 'resumeSince' < ${cutoff.toISOString()}`
        )
      )
      .for(`update`, { skipLocked: true })
    for (const row of rows) {
      await tx
        .update(sessions)
        .set({ needsInput: true, updatedAt: now })
        .where(eq(sessions.id, row.id))
      await busLine(tx, row, `session_interrupted`, BUS_COPY.session_unresumed, { waking: true })
    }
    return rows.length
  })
}

/** #EXP-1675 (incidents.json): a mid-turn `device_offline` whose device never
 *  came back (no `resumeSince`) `offlineMinutes` after the flag: on a person
 *  now, said once to its parent. `needs_input` is the once: `flagUnresumed`
 *  skips it too when the device returns, and whatever clears the flag (a beat
 *  listing the run, its report) clears Needs you with it. */
async function flagOfflineTooLong(now: Date): Promise<number> {
  const cutoff = new Date(now.getTime() - contract.sessionWatchdog.offlineMinutes * MIN)
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id, parentId: sessions.parentId })
      .from(sessions)
      .where(
        and(
          inArray(sessions.status, [...LIVE]),
          NOT_SUBAGENT,
          sql`${sessions.blocked} ->> 'kind' = 'device_offline'`,
          eq(sessions.needsInput, false),
          sql`(${sessions.blocked} ->> 'resume')::boolean IS TRUE`,
          sql`${sessions.blocked} ->> 'resumeSince' IS NULL`,
          sql`${sessions.blocked} ->> 'since' < ${cutoff.toISOString()}`
        )
      )
      .for(`update`, { skipLocked: true })
    for (const row of rows) {
      await tx
        .update(sessions)
        .set({ needsInput: true, updatedAt: now })
        .where(eq(sessions.id, row.id))
      await busLine(tx, row, `session_interrupted`, BUS_COPY.session_offline, { waking: true })
    }
    return rows.length
  })
}

/** FEED-129: a usage wall (`rate_limit`) whose reset time passed: cleared.
 *  The device clears a wall on the next assistant token, but a walled run
 *  whose engine went away (an account switch that failed, a host restart)
 *  has nobody to do it, and the row read "Usage limit" hours after the
 *  window reopened. A wall naming no reset stays the device's. Silent: the
 *  bus said the wall once; its passing is no line. */
async function clearExpiredWalls(now: Date): Promise<number> {
  return db.transaction(async (tx) => {
    const rows = await tx
      .select({ id: sessions.id, blocked: sessions.blocked })
      .from(sessions)
      .where(
        and(
          inArray(sessions.status, [...LIVE]),
          NOT_SUBAGENT,
          sql`${sessions.blocked} ->> 'kind' = 'rate_limit'`,
          sql`${sessions.blocked} ->> 'resetsAt' IS NOT NULL`
        )
      )
      .for(`update`, { skipLocked: true })
    // Parsed here, never cast in SQL: the stamp is a free string from the
    // device, and a malformed one must not fail the whole tick. The spec's
    // parser (#EXP-1510): a timezone-less stamp never expires.
    const expired = rows.filter((row) => {
      const at = stampMs(row.blocked?.resetsAt ?? ``)
      return Number.isFinite(at) && at <= now.getTime()
    })
    await clearBlockedInTx(
      tx,
      expired.map((row) => row.id),
      `rate_limit`
    )
    return expired.length
  })
}

/** FEED-146: the heartbeat's `liveSessionIds` is a free list from the
 *  device: only distinct uuids (lower-cased, as Postgres prints them) can
 *  name a session row; anything else is dropped, never cast in SQL (one bad
 *  id would fail every beat). FEED-148: a list of nothing but junk cleans
 *  to `[]`, which is still a claim ("I run nothing"): every busy session
 *  the sender hosts there reads dead. Accepted on purpose: only the host's
 *  own device can send it, and a broken client fails loudly, not silently
 *  keeps dead sessions busy. */
export function cleanLiveSessionIds(ids: readonly unknown[]): { ids: string[]; dropped: number } {
  const valid = ids.filter((id): id is string => typeof id === `string` && UUID.test(id))
  return { ids: [...new Set(valid.map((id) => id.toLowerCase()))], dropped: ids.length - valid.length }
}

/** The heartbeat's device: its sender and the machine id it beats for. */
export interface BeatingDevice {
  deviceId: string
  userId: string
}

/** FEED-147: the sessions a heartbeat may move: on that machine AND hosted
 *  by its sender (`hostedByCondition`), never every row naming the id.
 *  #EXP-1731: never a subagent row (never in the live list: its process is
 *  its parent's). */
function beatScope(device: BeatingDevice) {
  return and(eq(sessions.deviceId, device.deviceId), hostedByCondition(device.userId), NOT_SUBAGENT)!
}

/** The device heartbeat: its `device_offline` sessions are live again.
 *  EXP-1374: with the beat's `liveSessionIds` (null = an older device
 *  that never claims: every flag clears, as before) a listed session's
 *  process survived: the flag clears and a mid-turn one reads busy again;
 *  an unlisted mid-turn one keeps its flag, stamped `resumeSince`, until
 *  its device continues it (pickup `Continue`, whose report clears it). */
export async function restoreDeviceSessionsInTx(
  tx: Tx,
  device: BeatingDevice & { liveSessionIds?: readonly string[] | null },
  now: Date = new Date()
): Promise<number> {
  const rows = await tx
    .select({ id: sessions.id, blocked: sessions.blocked })
    .from(sessions)
    .where(and(beatScope(device), sql`${sessions.blocked} ->> 'kind' = 'device_offline'`))
    .for(`update`, { skipLocked: true })
  const live = device.liveSessionIds ? new Set(cleanLiveSessionIds(device.liveSessionIds).ids) : null
  const clear: string[] = []
  const busyAgain: string[] = []
  for (const row of rows) {
    const resume = row.blocked?.resume === true
    if (!live || !resume) clear.push(row.id)
    else if (live.has(row.id)) busyAgain.push(row.id)
    else if (!row.blocked?.resumeSince) {
      await tx
        .update(sessions)
        .set({ blocked: { ...row.blocked!, resumeSince: now.toISOString() }, updatedAt: now })
        .where(eq(sessions.id, row.id))
    }
  }
  if (busyAgain.length > 0) {
    await tx
      .update(sessions)
      // `background` resets with it: the device's next report says whether
      // only background tasks hold the run (never stale from before).
      .set({ status: `running`, busy: true, background: false, updatedAt: now })
      .where(inArray(sessions.id, busyAgain))
  }
  await clearBlockedInTx(tx, [...clear, ...busyAgain], `device_offline`)
  return clear.length + busyAgain.length
}

/** EXP-1374: the heartbeat's `sessionActivity` = each live session's
 *  latest engine activity (a tool call, terminal output): its sign of life
 *  for the stuck rule. Only the sender's hosted sessions, never forward of
 *  the beat itself (a skewed device clock cannot vouch for the future). */
export async function recordSessionActivityInTx(
  tx: Tx,
  device: BeatingDevice & { activity: readonly { sessionId: string; at: string }[] },
  now: Date = new Date()
): Promise<number> {
  const latest = new Map<string, number>()
  for (const entry of device.activity) {
    const id = cleanLiveSessionIds([entry.sessionId]).ids[0]
    const at = Date.parse(entry.at)
    if (!id || !Number.isFinite(at)) continue
    latest.set(id, Math.max(latest.get(id) ?? 0, Math.min(at, now.getTime())))
  }
  if (latest.size === 0) return 0
  // #EXP-1406: one statement however many sessions the beat lists.
  const beats = sql.join(
    [...latest].map(([id, at]) => sql`(${id}::uuid, ${new Date(at).toISOString()}::timestamptz)`),
    sql`, `
  )
  const moved = await tx
    .update(sessions)
    .set({ lastActivityAt: sql`beat.at` })
    .from(sql`(VALUES ${beats}) AS beat(id, at)`)
    .where(
      and(
        beatScope(device),
        sql`${sessions.id} = beat.id`,
        inArray(sessions.status, [...LIVE]),
        sql`COALESCE(${sessions.lastActivityAt}, '-infinity'::timestamptz) < beat.at`
      )
    )
    .returning({ id: sessions.id })
  return moved.length
}

/** EXP-1374: a sign of life the server saw itself (an MCP call by the
 *  session, a push to its branch): `last_activity_at`, throttled to one
 *  write a minute per session. Never `updated_at` (no sync traffic). */
export async function touchSessionActivity(where: SQL, now: Date = new Date()): Promise<void> {
  const step = new Date(now.getTime() - MIN)
  await db
    .update(sessions)
    .set({ lastActivityAt: now })
    .where(
      and(
        where,
        inArray(sessions.status, [...LIVE]),
        sql`COALESCE(${sessions.lastActivityAt}, '-infinity'::timestamptz) < ${step}`
      )
    )
}

/** FEED-145: a session the heartbeat lists in `liveSessionIds` is alive: a
 *  `stuck` flag on it (the 30-minute rule's, from before its device
 *  claimed) is cleared and `needs_input` re-derived from its open cards. */
export async function clearLiveStuckInTx(
  tx: Tx,
  device: BeatingDevice & { liveSessionIds: readonly string[] }
): Promise<number> {
  const live = cleanLiveSessionIds(device.liveSessionIds).ids
  if (live.length === 0) return 0
  const rows = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(
      and(
        beatScope(device),
        inArray(sessions.id, live),
        inArray(sessions.status, [...LIVE]),
        sql`${sessions.blocked} ->> 'kind' = 'stuck'`
      )
    )
    .for(`update`, { skipLocked: true })
  await clearBlockedInTx(
    tx,
    rows.map((row) => row.id),
    `stuck`
  )
  return rows.length
}

/** NEW-5: the heartbeat's `liveSessionIds` = the sessions the host process
 *  runs. A `running` + `busy` session on that device that is NOT listed has
 *  a dead process (the daemon lives, the run does not): idle, not busy.
 *  EXP-1374: an INTERRUPTION (`restarted`, mid-turn): its device continues
 *  it (pickup `Continue`), never a person's; S-3: silent (no bus line). */
export async function flagDeadSessionsInTx(
  tx: Tx,
  device: BeatingDevice & { liveSessionIds: readonly string[] },
  now: Date = new Date()
): Promise<number> {
  const live = cleanLiveSessionIds(device.liveSessionIds).ids
  const rows = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(
      and(
        beatScope(device),
        eq(sessions.status, `running`),
        eq(sessions.busy, true),
        ...(live.length > 0 ? [notInArray(sessions.id, live)] : [])
      )
    )
    .for(`update`, { skipLocked: true })
  for (const row of rows) {
    await tx
      .update(sessions)
      .set({
        status: `idle`,
        busy: false,
        background: false,
        blocked: interruption(`restarted`, now, true, now),
        updatedAt: now,
      })
      .where(eq(sessions.id, row.id))
  }
  return rows.length
}

export async function runWatchdog(now: Date = new Date()): Promise<WatchdogRun> {
  return {
    unclaimed: await flagUnclaimed(now),
    nudged: await nudgeStalled(now),
    undelivered: await flagUndelivered(now),
    stuck: await flagStuck(now),
    spoke: await restoreSpoken(now),
    offline: await flagOffline(now),
    unresumed: await flagUnresumed(now),
    offlineLong: await flagOfflineTooLong(now),
    expired: await clearExpiredWalls(now),
    // NEW-2: the same tick fires the agents' schedules (a missed tick is
    // caught up here) and sweeps the event stream.
    fired: await fireScheduledTriggers(now),
    pruned: await pruneEvents(now),
    meetingPruned: await pruneExpiredMessages(now),
    meetingsStopped: await stopExpiredMeetings(now),
    // VAPP-159: app records trashed past the retention (app-records.json `purge`).
    recordsPurged: await purgeTrashedAppRecords(now),
    // VAPP-215: record history past its 90 days (app-records.json `history`).
    recordEventsPurged: await purgeOldAppRecordEvents(now),
    // eApps R8 (#VAPP-233): resume every running publish sweep.
    recordsMigrated: await import(`@/lib/apps/record-sweep`).then((m) => m.runRecordSweeps()).catch((err) => {
      console.error(`[watchdog] record sweep failed`, err)
      return 0
    }),
    // #VAPP-220: the apps' schedule automations (caught up like the agents').
    // Loaded lazily: the executor pulls in the broker and the MCP server.
    automations: await import(`@/lib/apps/automations`).then((m) => m.fireScheduledAutomations(now)).catch((err) => {
      console.error(`[watchdog] automations failed`, err)
      return 0
    }),
    // #VAPP-237: webhook nonces past the replay window.
    hookNoncesPruned: await import(`@/lib/apps/webhooks`).then((m) => m.pruneHookNonces(now)).catch((err) => {
      console.error(`[watchdog] hook nonces failed`, err)
      return 0
    }),
    // #VAPP-239: device-routed upstream calls a crashed request left behind.
    deviceCallsPruned: await import(`@/lib/emcp/upstream/device-call`).then((m) => m.pruneDeviceCalls(now)).catch((err) => {
      console.error(`[watchdog] device calls failed`, err)
      return 0
    }),
    // EXP-1610: a CI check queued past contract prChecks.queuedFlagMinutes
    // wakes its PR's session once (no runner picked it up).
    checksQueued: await flagQueuedChecks(now),
    // Batch 3: CI wakes without the check webhooks (pr-checks.json SWEEP).
    checksSwept: await sweepUnsettledChecks(now).catch((err) => {
      console.error(`[watchdog] checks sweep failed`, err)
      return 0
    }),
    // #EXP-1617 (eCode runner.json `tick`): our CI's clock, then dispatch.
    ciJobs: await import(`@/lib/ci/runs`)
      .then((m) => m.ciTick(now))
      .catch((err) => {
        console.error(`[watchdog] CI tick failed`, err)
        return 0
      }),
    ciLogsPruned: await import(`@/lib/ci/logs`)
      .then((m) => m.sweepOrphanLogs(now))
      .catch((err) => {
        console.error(`[watchdog] CI log sweep failed`, err)
        return 0
      }),
    // EXP-1706 (issue-lifecycle.json `reconcile`): a lost `closed` delivery
    // never strands a merged PR (cloud too, not only GITHUB_POLLING).
    // EXP-1709 (merge-train.json): the merge train's tick and the
    // per-commit checks it no longer needs.
    mergeTrainRepos: await import(`@/lib/merge-train/worker`)
      .then((m) => m.runMergeTrainTick(now))
      .catch((err) => {
        console.error(`[watchdog] merge train failed`, err)
        return 0
      }),
    commitChecksPruned: await import(`@/lib/merge-train/commit-checks`)
      .then((m) => m.pruneCommitChecks(now))
      .then(() => 1)
      .catch((err) => {
        console.error(`[watchdog] commit checks prune failed`, err)
        return 0
      }),
    ecodeUploadsSwept: await import(`@/lib/ecode/git-store`)
      .then((m) => m.sweepOrphanUploads(now))
      .catch((err) => {
        console.error(`[watchdog] eCode upload sweep failed`, err)
        return 0
      }),
    prsReconciled: await import(`@/lib/integrations/pr-reconcile`)
      .then((m) => m.reconcileStaleLivePullRequests(now))
      .catch((err) => {
        console.error(`[watchdog] PR reconcile failed`, err)
        return 0
      }),
    // #EXP-1797 (issue-lifecycle.json `trash`): issues trashed past the
    // retention window are purged (cascade + their files).
    issuesPurged: await import(`@/lib/issue-trash`)
      .then((m) => m.runIssuePurgeSweep(now))
      .catch((err) => {
        console.error(`[watchdog] issue trash purge failed`, err)
        return 0
      }),
  }
}

/** NEW-57: one tick deletes at most this many expired messages (the rest
 *  go on the next ticks). */
export const EXPIRED_MESSAGES_BATCH = 500

/** NEW-57 (meeting.json RESIDENCY): delete the messages whose `expires_at`
 *  passed (meeting transcript batches, `contract.meeting.retentionDays`),
 *  oldest first, one bounded batch per tick over `idx_messages_expires`.
 *  The statement-level `refresh_conversation_digests_delete` trigger
 *  recomputes each touched conversation's digest; attachments cascade. */
export async function pruneExpiredMessages(now: Date = new Date()): Promise<number> {
  const expired = db
    .select({ id: messages.id })
    .from(messages)
    .where(and(isNotNull(messages.expiresAt), lt(messages.expiresAt, now)))
    .orderBy(asc(messages.expiresAt))
    .limit(EXPIRED_MESSAGES_BATCH)
  const gone = await db
    .delete(messages)
    .where(inArray(messages.id, expired))
    .returning({ id: messages.id })
  return gone.length
}

/** NEW-57 AUTO-STOP (watchdog): clear every meeting started at least the
 *  member's `meeting_auto_stop_hours` ago (FEED-163; a value outside
 *  `autoStop.hoursValues` = the default) and nudge its voice device (which
 *  stops on its own clock too). FEED-163 NOTE: the UPDATE is the conditional
 *  clear (a row the device's `stopMeeting {reason: 'autoStop'}` cleared first
 *  no longer matches), so each row it returns posts the autoStop note once.
 *  Returns how many stopped. */
export async function stopExpiredMeetings(now: Date = new Date()): Promise<number> {
  const hours = sql`CASE WHEN ${userVoiceSettings.meetingAutoStopHours} IN (${sql.join(
    MEETING_AUTO_STOP.hoursValues.map((value) => sql`${value}::int`),
    sql`, `
  )}) THEN ${userVoiceSettings.meetingAutoStopHours} ELSE ${MEETING_AUTO_STOP.defaultHours}::int END`
  const stopped = await db
    .update(userVoiceSettings)
    .set({ meetingStartedAt: null, updatedAt: now })
    .where(
      and(
        isNotNull(userVoiceSettings.meetingStartedAt),
        sql`${userVoiceSettings.meetingStartedAt} <= ${now.toISOString()}::timestamptz - make_interval(hours => ${hours})`
      )
    )
    .returning({
      userId: userVoiceSettings.userId,
      voiceDeviceId: userVoiceSettings.voiceDeviceId,
      agentId: userVoiceSettings.agentId,
      hours: userVoiceSettings.meetingAutoStopHours,
    })
  const config = getSteerRelayConfig()
  for (const row of stopped) {
    if (config && row.voiceDeviceId) void relayPostNudge(config, row.userId, row.voiceDeviceId).catch(() => {})
    if (row.agentId) {
      await postMeetingStopNote(db, row.userId, row.agentId, `autoStop`, autoStopHours(row.hours)).catch(
        (error: unknown) => console.error(`[meeting] auto-stop note failed`, error)
      )
    }
  }
  return stopped.length
}

let started = false

export function startSessionWatchdog(): void {
  if (started) return
  started = true
  setInterval(() => {
    const begun = performance.now()
    runWatchdog()
      .then((run) =>
        reportSchedulerRun(`session-watchdog`, {
          ok: true,
          durationMs: performance.now() - begun,
          detail: `${run.unclaimed} unclaimed, ${run.nudged} nudged, ${run.undelivered} undelivered, ${run.stuck} stuck, ${run.spoke} spoke again, ${run.offline} offline, ${run.unresumed} unresumed, ${run.offlineLong} offline too long, ${run.expired} walls expired, ${run.fired} fired, ${run.pruned} events pruned, ${run.meetingPruned} meeting messages pruned, ${run.meetingsStopped} meetings auto-stopped, ${run.recordsPurged} app records purged, ${run.recordEventsPurged} record events purged, ${run.recordsMigrated} records migrated, ${run.checksQueued} queued CI checks flagged`,
        })
      )
      .catch((error: unknown) => {
        reportSchedulerRun(`session-watchdog`, {
          ok: false,
          durationMs: performance.now() - begun,
          error: String(error),
        })
        console.error(`[session-watchdog] failed:`, error)
      })
  }, WATCHDOG_INTERVAL_MS)
}

