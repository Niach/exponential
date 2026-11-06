// #EXP-1639: a running session on the lock screen (Live Activity, Android
// Live Update). Pure and locked by `fixtures/live-activity.json`, replayed by
// iOS ExpPush `LiveActivityModel.swift` and Android
// `data/push/LiveActivity.kt`. The fixture's `$comment` holds the rules;
// this is the reference.

import fixture from "../fixtures/live-activity.json" with { type: "json" }

export const LIVE_ACTIVITY = fixture

export type LiveActivityPhase = keyof typeof fixture.copy.phases

export interface LiveActivitySession {
  id: string
  parentId: string | null
  userId: string
  agentId: string | null
  startedByAgentId: string | null
  /** The display title; ignored when `sealedTitle`. */
  title: string | null
  sealedTitle: boolean
  status: string
  busy: boolean
  needsInput: boolean
  blocked: boolean
  startedAt: string | null
}

export interface LiveActivityContent {
  phase: LiveActivityPhase
  title: string
  subtitle: string
  detail: string | null
  /** `needs-you`, `PR #<n>` or `timer`. */
  badge: string
  segment: number
}

export function liveActivityPhase(session: Pick<LiveActivitySession, `status` | `needsInput` | `blocked`>): LiveActivityPhase {
  if (session.needsInput) return `needsYou`
  if (session.status === `idle` || session.status === `ended`) return `done`
  if (session.blocked) return `blocked`
  return session.status === `queued` ? `starting` : `working`
}

/** The sessions a member's phone follows, newest started first, capped. */
export function liveActivityFollows(input: { me: string; sessions: LiveActivitySession[] }): string[] {
  return input.sessions
    .filter(
      (session) =>
        session.parentId === null &&
        session.userId === input.me &&
        session.startedByAgentId === null &&
        session.agentId === null &&
        liveActivityPhase(session) !== `done`
    )
    .sort((a, b) => (b.startedAt ?? ``).localeCompare(a.startedAt ?? ``))
    .slice(0, fixture.maxActivities)
    .map((session) => session.id)
}

/** What the activity shows. */
export function liveActivityContent(input: {
  session: LiveActivitySession
  deviceLabel: string | null
  prNumber: number | null
  askPreview: string | null
  blockedLabel: string | null
}): LiveActivityContent {
  const phase = liveActivityPhase(input.session)
  const label = fixture.copy.phases[phase]
  const pr = input.prNumber !== null ? fixture.copy.prBadge.replace(`{n}`, String(input.prNumber)) : null
  return {
    phase,
    title: input.session.sealedTitle || !input.session.title ? fixture.copy.encryptedTitle : input.session.title,
    subtitle: input.deviceLabel ? `${label} · ${input.deviceLabel}` : label,
    detail: phase === `needsYou` ? input.askPreview : phase === `blocked` ? input.blockedLabel : null,
    badge: phase === `needsYou` ? `needs-you` : (pr ?? `timer`),
    segment: phase === `done` ? 4 : phase === `needsYou` ? 2 : pr ? 3 : 1,
  }
}
