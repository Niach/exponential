// EXP-1341/NEW-2: ONE session model ×4. A thread and an agent's conversation
// run the same way; a conversation session (a root with `agentId`) is just
// `persistent` (never ends, revived on its device's return). Branch on these
// traits, never on the kind string: `sessionKindOf` derives the kind from
// `agentId`.
// fixtures/session-kind.json; mirrors: desktop `domain::session_kind`, iOS
// `SessionKind.swift`, Android `domain/SessionKind.kt`.

import fixture from "../fixtures/session-kind.json" with { type: "json" }

export interface SessionTraits {
  /** Never ends: exits idle, `sessions.end` refuses, revived by its device. */
  persistent: boolean
  /** The kind whose definition carries triggers (the server fires them). */
  triggers: boolean
}

/** The kinds the contract knows. */
export type SessionKindName = keyof typeof fixture.kinds

const KINDS = fixture.kinds as Record<SessionKindName, SessionTraits>

export function sessionTraits(kind: string | null | undefined): SessionTraits {
  return (kind && (KINDS as Record<string, SessionTraits>)[kind]) || KINDS.thread
}

/** NEW-2: the kind of a session row: `agent` when it is an agent's
 *  conversation session (`agentId` set), else `thread`. */
export function sessionKindOf(session: { agentId?: string | null }): SessionKindName {
  return session.agentId ? `agent` : `thread`
}

/** A persistent session (an agent's conversation): it idles instead of ending. */
export const isPersistent = (session: { agentId?: string | null }): boolean =>
  sessionTraits(sessionKindOf(session)).persistent

/** The kinds that never end. */
export const PERSISTENT_KINDS = (Object.keys(KINDS) as SessionKindName[]).filter(
  (kind) => KINDS[kind].persistent
)

/** The kinds whose definition carries triggers. */
export const TRIGGER_KINDS = (Object.keys(KINDS) as SessionKindName[]).filter(
  (kind) => KINDS[kind].triggers
)
