// #EXP-1456: where an inbox row (and its push) opens ×4. An issue row opens
// its issue; an issue-less row naming a session opens that session (an
// agent's question or plan, a mention, a wall); #EXP-1534: an `agent_signed_out`
// row opens its device's settings; `sessionTarget` is the
// session entity ref's rule. fixtures/inbox-route.json; mirrors: desktop
// `domain::inbox_route`, iOS `InboxRoute.swift`, Android `domain/InboxRoute.kt`.

export interface InboxRouteRow {
  type: string
  issueId: string | null
  sessionId: string | null
  /** #EXP-1534: the devices row an `agent_signed_out` row is about. */
  deviceId?: string | null
}

export type InboxRowTarget =
  | { kind: `issue`; issueId: string; face: `results` | null }
  | { kind: `session`; sessionId: string }
  | { kind: `device`; deviceId: string }
  | { kind: `none` }

/** The issue-less types an inbox lists. */
const SESSION_ROW_TYPES = new Set([`agent_message`, `session_blocked`])

export function inboxRowTarget(row: InboxRouteRow): InboxRowTarget {
  if (row.issueId) {
    return {
      kind: `issue`,
      issueId: row.issueId,
      face: row.type === `agent_message` ? `results` : null,
    }
  }
  if (SESSION_ROW_TYPES.has(row.type) && row.sessionId) {
    return { kind: `session`, sessionId: row.sessionId }
  }
  if (row.type === `agent_signed_out` && row.deviceId) {
    return { kind: `device`, deviceId: row.deviceId }
  }
  return { kind: `none` }
}

export type SessionTarget =
  | { kind: `agent`; agentId: string }
  | { kind: `chat`; chatId: string; sessionId: string | null }
  | { kind: `session`; sessionId: string }

/** Where a session opens (S-3): the viewer's own agent conversation root =
 *  the agent's page, a chat's root or child = the chat, anything else
 *  under an agent = the session itself. `agentId` + `userId` are the root's. */
export function sessionTarget(
  session: { id: string; rootId: string | null; agentId: string | null; userId: string | null },
  viewerId: string | null
): SessionTarget {
  const rootId = session.rootId ?? session.id
  if (session.agentId) {
    return rootId === session.id && session.userId === viewerId
      ? { kind: `agent`, agentId: session.agentId }
      : { kind: `session`, sessionId: session.id }
  }
  return { kind: `chat`, chatId: rootId, sessionId: rootId === session.id ? null : session.id }
}
