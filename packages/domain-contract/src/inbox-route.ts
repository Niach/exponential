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

/** #EXP-1987: whether team `teamId`'s inbox (null = every team) lists a row,
 *  its badge and Mark all read alike. `row.teamId` = the row's own team, else
 *  its issue's board's team (null = unknown); `teamIds` = the synced teams
 *  rows (never a personal team). `agent_signed_out` and a session row of a
 *  team the viewer does not sync (the Assistant's chat in the personal team)
 *  list in every team. fixtures/inbox-route.json `listCases`. */
export function inboxRowListed(
  row: { type: string; teamId: string | null; issueId: string | null },
  teamId: string | null,
  teamIds: readonly string[]
): boolean {
  if (row.issueId) return row.teamId !== null && (teamId === null || row.teamId === teamId)
  if (row.type === `agent_signed_out`) return true
  if (!SESSION_ROW_TYPES.has(row.type)) return false
  if (teamId === null) return true
  return row.teamId === teamId || row.teamId === null || !teamIds.includes(row.teamId)
}

export type SessionTarget =
  | { kind: `agent`; agentId: string }
  | { kind: `chat`; chatId: string; sessionId: string | null }
  | { kind: `session`; sessionId: string }

/** Where a session opens (S-3): the viewer's own agent conversation root =
 *  the agent's page, a chat's root or child = the chat, anything else
 *  under an agent = the session itself. `agentId` + `userId` are the root's.
 *  #EXP-1990: a team agent's group chat (`groupChat`: the root is its
 *  agent's `agents.groupChatId`) is every member's chat with it. */
export function sessionTarget(
  session: { id: string; rootId: string | null; agentId: string | null; userId: string | null; groupChat?: boolean },
  viewerId: string | null
): SessionTarget {
  const rootId = session.rootId ?? session.id
  if (session.agentId) {
    return rootId === session.id && (session.userId === viewerId || session.groupChat === true)
      ? { kind: `agent`, agentId: session.agentId }
      : { kind: `session`, sessionId: session.id }
  }
  return { kind: `chat`, chatId: rootId, sessionId: rootId === session.id ? null : session.id }
}
