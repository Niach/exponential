// EXP-1341: ONE session model ×4. A thread and an agent run the same way;
// an agent is just `persistent` (never ends, revived on its device's return)
// and carries `triggers`. Branch on these traits, never on the kind string.
// fixtures/session-kind.json; mirrors: desktop `domain::session_kind`, iOS
// `SessionKind.swift`, Android `domain/SessionKind.kt`.

import fixture from "../fixtures/session-kind.json" with { type: "json" }

export interface SessionTraits {
  /** Never ends: exits idle, `sessions.end` refuses, revived by its device. */
  persistent: boolean
  /** Carries triggers (schedules / events) its device fires as messages. */
  triggers: boolean
}

/** The kinds the contract knows (the DB enum's values). */
export type SessionKindName = keyof typeof fixture.kinds

const KINDS = fixture.kinds as Record<SessionKindName, SessionTraits>

export function sessionTraits(kind: string | null | undefined): SessionTraits {
  return (kind && (KINDS as Record<string, SessionTraits>)[kind]) || KINDS.thread
}

/** A persistent session (an agent root): it idles instead of ending. */
export const isPersistent = (session: { kind?: string | null }): boolean =>
  sessionTraits(session.kind).persistent

/** The kinds that never end (SQL filters: `NOT IN` = the ones that do). */
export const PERSISTENT_KINDS = (Object.keys(KINDS) as SessionKindName[]).filter(
  (kind) => KINDS[kind].persistent
)

/** The kinds that carry triggers. */
export const TRIGGER_KINDS = (Object.keys(KINDS) as SessionKindName[]).filter(
  (kind) => KINDS[kind].triggers
)
