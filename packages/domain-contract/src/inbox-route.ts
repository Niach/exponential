// #EXP-1456: where an inbox row (and its push) opens ×4. An issue row opens
// its issue; an issue-less row naming a session opens that session (an
// agent's question or plan, a mention, a wall); `sessionTarget` is the
// session entity ref's rule. fixtures/inbox-route.json; mirrors: desktop
// `domain::inbox_route`, iOS `InboxRoute.swift`, Android `domain/InboxRoute.kt`.

export interface InboxRouteRow {
  type: string
  issueId: string | null
  sessionId: string | null
}

export type InboxRowTarget =
  | { kind: `issue`; issueId: string; face: `results` | null }
  | { kind: `session`; sessionId: string }
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
  return { kind: `none` }
}

export type SessionTarget =
  | { kind: `agent`; agentId: string }
  | { kind: `thread`; threadId: string; sessionId: string | null }
  | { kind: `session`; sessionId: string }

/** Where a session opens (S-3): the viewer's own agent conversation root =
 *  the agent's page, a thread's root or child = the thread, anything else
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
  return { kind: `thread`, threadId: rootId, sessionId: rootId === session.id ? null : session.id }
}
