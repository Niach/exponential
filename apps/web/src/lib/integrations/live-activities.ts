import { and, desc, eq, inArray, isNotNull, isNull, or, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  conversations,
  devices,
  fcmTokens,
  liveActivityFollows,
  messages,
  pullRequests,
  sessions,
} from "@/db/schema"
import { getRelayUrl, sealedPlaintext } from "@/lib/integrations/fcm"
import { pushSeal } from "@/lib/push-seal"
import { reportSchedulerRun } from "@/lib/metrics/registry"
import { blockedBadgeLabel } from "@/lib/agent-usage"
import { deriveSessionTitle } from "@/lib/sessions/title"
import { askText as busAskText } from "@/lib/sessions/bus"
import { isSealed, isSealedCard } from "@/lib/sessions/sealed"
import {
  LIVE_ACTIVITY,
  liveActivityContent,
  liveActivityFollows as followedSessionIds,
  type LiveActivityContent,
  type LiveActivitySession,
} from "@exp/domain-contract/live-activity"
import { questionPushText } from "@exp/domain-contract/question-push"
import type { CodingSessionBlocked, MessageCard } from "@exp/db-schema/domain"

// #EXP-1639 (live-activity.json): a running session on the phone's lock
// screen. ONE sync decides, per member with a Live-Activity-capable push
// registration, which sessions the phone follows and what each shows; a
// change goes to push-relay's POST /activity SEALED to the phone's push key
// (the relay and Apple see ciphertext only). Runs every `TICK_MS` and on a
// kick (an ask). Best effort, never throws.

/** The `data.type` of an activity payload (the phones route on it). */
export const LIVE_ACTIVITY_PUSH_TYPE = `session_activity`
const TICK_MS = LIVE_ACTIVITY.minUpdateSeconds * 1000
const RELAY_TIMEOUT_MS = 10_000

type Event = `start` | `update` | `end`

interface TokenRow {
  id: string
  userId: string
  platform: string
  capability: string
  pushPublicKey: string
}

interface SessionRow extends LiveActivitySession {
  deviceId: string
  rootId: string
  blockedRaw: CodingSessionBlocked | null
}

/** The sealed plaintext of one activity: the push shape every phone opens. */
export function activityPayload(
  session: Pick<SessionRow, `id` | `startedAt`>,
  content: LiveActivityContent,
  userId: string
): { title: string; body: string; data: Record<string, string> } {
  return {
    title: content.title,
    body: content.subtitle,
    data: {
      type: LIVE_ACTIVITY_PUSH_TYPE,
      sessionId: session.id,
      phase: content.phase,
      badge: content.badge,
      segment: String(content.segment),
      ...(content.detail ? { detail: content.detail } : {}),
      ...(session.startedAt ? { startedAt: session.startedAt } : {}),
      userId,
    },
  }
}

/** Whether a changed content goes out now: a phase change to or from
 *  `needsYou`/`done` always, anything else once per `minUpdateSeconds`. */
export function shouldSend(previous: string, next: string, sentAt: Date, now: Date): boolean {
  if (previous === next) return false
  // A follow the phone just registered (empty) gets the current content.
  if (previous === ``) return true
  const phase = (raw: string) => {
    try {
      return (JSON.parse(raw) as { data?: { phase?: string } }).data?.phase ?? null
    } catch {
      return null
    }
  }
  const before = phase(previous)
  const after = phase(next)
  if (before !== after && (after === `needsYou` || after === `done` || before === `needsYou`)) return true
  return now.getTime() - sentAt.getTime() >= TICK_MS
}

async function loadSessions(userIds: string[], followedIds: string[]): Promise<SessionRow[]> {
  const rows = await db
    .select({
      id: sessions.id,
      rootId: sessions.rootId,
      parentId: sessions.parentId,
      userId: sessions.userId,
      agentId: sessions.agentId,
      startedByAgentId: sessions.startedByAgentId,
      title: sessions.title,
      status: sessions.status,
      busy: sessions.busy,
      needsInput: sessions.needsInput,
      blocked: sessions.blocked,
      startedAt: sessions.startedAt,
      createdAt: sessions.createdAt,
      deviceId: sessions.deviceId,
    })
    .from(sessions)
    .where(
      or(
        and(
          inArray(sessions.userId, userIds),
          isNull(sessions.parentId),
          isNull(sessions.agentId),
          isNull(sessions.startedByAgentId),
          or(inArray(sessions.status, [`queued`, `running`]), eq(sessions.needsInput, true))
        ),
        followedIds.length ? inArray(sessions.id, followedIds) : sql`false`
      )
    )
  return rows.map((row) => ({
    ...row,
    sealedTitle: isSealed(row.title),
    blocked: row.blocked !== null,
    blockedRaw: row.blocked,
    startedAt: (row.startedAt ?? row.createdAt).toISOString(),
  }))
}

async function contentFor(session: SessionRow, now: Date): Promise<LiveActivityContent> {
  const [device] = await db
    .select({ label: devices.label })
    .from(devices)
    .where(and(eq(devices.userId, session.userId), eq(devices.deviceId, session.deviceId)))
    .limit(1)
  const [pr] = await db
    .select({ number: pullRequests.number })
    .from(pullRequests)
    .where(
      and(
        or(eq(pullRequests.sessionId, session.id), eq(pullRequests.rootSessionId, session.id)),
        eq(pullRequests.state, `open`)
      )
    )
    .orderBy(desc(pullRequests.createdAt))
    .limit(1)
  let askPreview: string | null = null
  if (session.needsInput) {
    const [ask] = await db
      .select({ card: messages.card })
      .from(messages)
      .innerJoin(conversations, eq(conversations.id, messages.conversationId))
      .where(and(eq(conversations.sessionId, session.id), isNotNull(messages.card), isNull(messages.answeredAt)))
      .orderBy(desc(messages.createdAt))
      .limit(1)
    const card = ask?.card as MessageCard | null | undefined
    if (card && (card.kind === `question` || card.kind === `plan`) && !isSealedCard(card)) {
      askPreview = questionPushText(busAskText(session, card))
    }
  }
  const title = session.sealedTitle
    ? null
    : await deriveSessionTitle({ id: session.id, rootId: session.rootId, title: session.title })
  return liveActivityContent({
    session: { ...session, title },
    deviceLabel: device?.label ?? null,
    prNumber: pr?.number ?? null,
    askPreview,
    blockedLabel: blockedBadgeLabel(session.blockedRaw, now),
  })
}

async function postActivity(
  url: string,
  token: TokenRow,
  event: Event,
  payload: ReturnType<typeof activityPayload>,
  extra: { activityId?: string | null; urgency: `high` | `normal`; dismissAt?: Date }
): Promise<number> {
  const sealed = pushSeal(
    Buffer.from(token.pushPublicKey, `base64url`),
    sealedPlaintext(payload.title, payload.body, payload.data)
  )
  const res = await fetch(`${url}/activity`, {
    method: `POST`,
    headers: { "Content-Type": `application/json`, Authorization: `Bearer ${token.capability}` },
    signal: AbortSignal.timeout(RELAY_TIMEOUT_MS),
    body: JSON.stringify({
      event,
      sealed: Buffer.from(sealed).toString(`base64url`),
      urgency: extra.urgency,
      ...(extra.activityId ? { activityId: extra.activityId } : {}),
      ...(extra.dismissAt ? { dismissAt: extra.dismissAt.toISOString() } : {}),
    }),
  })
  return res.status
}

/** One sync over the given members (all with a capable registration when
 *  omitted). Returns how many activity pushes went out. */
export async function syncLiveActivities(userIds?: string[], now: Date = new Date()): Promise<number> {
  const url = getRelayUrl()
  if (!url) return 0
  const tokens = (await db
    .select({
      id: fcmTokens.id,
      userId: fcmTokens.userId,
      platform: fcmTokens.platform,
      capability: fcmTokens.relayCapability,
      pushPublicKey: fcmTokens.pushPublicKey,
    })
    .from(fcmTokens)
    .where(
      and(
        eq(fcmTokens.liveActivities, true),
        isNotNull(fcmTokens.relayCapability),
        isNotNull(fcmTokens.pushPublicKey),
        userIds ? inArray(fcmTokens.userId, userIds) : undefined
      )
    )) as TokenRow[]
  if (tokens.length === 0) return 0
  const follows = await db
    .select()
    .from(liveActivityFollows)
    .where(inArray(liveActivityFollows.fcmTokenId, tokens.map((token) => token.id)))
  const members = [...new Set(tokens.map((token) => token.userId))]
  const rows = await loadSessions(members, [...new Set(follows.map((follow) => follow.sessionId))])
  const byId = new Map(rows.map((row) => [row.id, row]))
  const contents = new Map<string, LiveActivityContent>()
  const content = async (session: SessionRow) => {
    const known = contents.get(session.id)
    if (known) return known
    const fresh = await contentFor(session, now)
    contents.set(session.id, fresh)
    return fresh
  }

  let sent = 0
  for (const token of tokens) {
    const followed = new Set(followedSessionIds({ me: token.userId, sessions: rows }))
    const mine = follows.filter((follow) => follow.fcmTokenId === token.id)
    const ios = token.platform === `ios`
    try {
      // Ended follows first: their last content stays `dismissMinutes`.
      for (const follow of mine) {
        if (followed.has(follow.sessionId)) continue
        const session = byId.get(follow.sessionId)
        await db.delete(liveActivityFollows).where(eq(liveActivityFollows.id, follow.id))
        if (!session || (ios && !follow.relayActivityId)) continue
        const payload = activityPayload(session, await content(session), token.userId)
        await postActivity(url, token, `end`, payload, {
          activityId: follow.relayActivityId,
          urgency: `high`,
          dismissAt: new Date(now.getTime() + LIVE_ACTIVITY.dismissMinutes * 60_000),
        })
        sent += 1
      }
      for (const sessionId of followed) {
        const session = byId.get(sessionId)!
        const next = await content(session)
        const payload = activityPayload(session, next, token.userId)
        const serialized = JSON.stringify(payload)
        const follow = mine.find((row) => row.sessionId === sessionId)
        const urgency = next.phase === `needsYou` ? `high` : `normal`
        if (!follow) {
          const status = await postActivity(url, token, ios ? `start` : `update`, payload, { urgency })
          await db
            .insert(liveActivityFollows)
            .values({ sessionId, fcmTokenId: token.id, content: serialized, sentAt: now })
            .onConflictDoNothing()
          if (status < 300) sent += 1
          continue
        }
        // iOS updates need the activity's own id, which the phone registers
        // once ActivityKit started it (or after the member dismissed it: null).
        if (ios && !follow.relayActivityId) continue
        if (!shouldSend(follow.content, serialized, follow.sentAt, now)) continue
        const status = await postActivity(url, token, `update`, payload, { activityId: follow.relayActivityId, urgency })
        await db
          .update(liveActivityFollows)
          .set({
            content: serialized,
            sentAt: now,
            updatedAt: now,
            // 410: the member dismissed it; it stays followed, unpushed.
            ...(status === 410 ? { relayActivityId: null } : {}),
          })
          .where(eq(liveActivityFollows.id, follow.id))
        if (status < 300) sent += 1
      }
    } catch (err) {
      console.error(`[live-activities] sync for a registration failed:`, err)
    }
  }
  return sent
}

/** A best-effort immediate sync for these members (an ask, a PR). */
export function kickLiveActivities(userIds: string[]): void {
  if (userIds.length === 0) return
  void syncLiveActivities(userIds).catch((err: unknown) => console.error(`[live-activities] kick failed:`, err))
}

let started = false
let running = false

export function startLiveActivityScheduler(): void {
  if (started) return
  started = true
  setInterval(() => {
    if (running) return
    running = true
    const begun = performance.now()
    syncLiveActivities()
      .then((count) =>
        reportSchedulerRun(`live-activities`, { ok: true, durationMs: performance.now() - begun, detail: `${count} sent` })
      )
      .catch((error: unknown) =>
        reportSchedulerRun(`live-activities`, { ok: false, durationMs: performance.now() - begun, error: String(error) })
      )
      .finally(() => {
        running = false
      })
  }, TICK_MS).unref?.()
}
