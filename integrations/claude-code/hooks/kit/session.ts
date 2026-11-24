// GENERATED from packages/plugin-kit/src/model/core/session.ts (bun run plugin-kit:generate). Do not edit.
import type { IssueSession } from "./wire"
import type { Tone } from "./issue"

// A live session's one-line state, the words the clients' session rows use
// (session-display.json: needs input beats working).

export type SessionState = `needs_input` | `working` | `queued` | `idle`

export function sessionState(row: Pick<IssueSession, `status` | `busy` | `needsInput`>): SessionState {
  if (row.needsInput) return `needs_input`
  if (row.busy) return `working`
  return row.status === `queued` ? `queued` : `idle`
}

export const SESSION_STATE_LABEL: Record<SessionState, string> = {
  needs_input: `Needs input`,
  working: `Working`,
  queued: `Queued`,
  idle: `Idle`,
}

export const SESSION_STATE_TONE: Record<SessionState, Tone> = {
  needs_input: `yellow`,
  working: `neutral`,
  queued: `neutral`,
  idle: `blue`,
}

export const SESSION_STATE_GLYPH: Record<SessionState, string> = {
  needs_input: `●`,
  working: `◐`,
  queued: `○`,
  idle: `●`,
}

/** The session that says what the issue's run is doing: one that waits on a
 *  person first, then a working one, else the latest. */
export function leadingSession(rows: readonly IssueSession[] | null | undefined): IssueSession | null {
  if (!rows?.length) return null
  return rows.find((row) => row.needsInput) ?? rows.find((row) => row.busy) ?? rows[0]!
}
