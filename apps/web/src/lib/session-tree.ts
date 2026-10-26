// EXP-1293 Threads v2 (web): the session tree's view models are the
// contract's (`@exp/domain-contract/session-tree`, fixture-locked ×4); this
// re-exports them and maps the synced collection rows onto their inputs.

import type { z } from "zod"
import type {
  selectPullRequestSchema,
  selectSyncedMessageSchema,
  selectSyncedSessionIssueSchema,
  selectSyncedSessionSchema,
} from "@exp/db-schema/schema"
import { keyMessagesBySession } from "@exp/domain-contract/message-conversation"
import type {
  TreeIssue,
  TreeMessage,
  TreePullRequest,
  TreeSession,
  TreeSessionIssue,
} from "@exp/domain-contract/session-tree"

export * from "@exp/domain-contract/session-tree"
export {
  conversationSessions,
  keyMessagesBySession,
  type MessageConversation,
} from "@exp/domain-contract/message-conversation"
export {
  guideViewer,
  type GuideViewerEntry,
  type GuideViewerState,
} from "@exp/domain-contract/guide-viewer"
export {
  AGENT_SETTINGS_COPY,
  AGENT_SETTINGS_FIELDS,
  agentAccountValue,
  triggerSummary,
  validateAgentForm,
  type AgentForm,
  type AgentFormError,
  type AgentTriggerLike,
} from "@exp/domain-contract/agent-settings"

/** NEW-8: the legacy session columns step 4 drops. The server still serves
 *  them today; the client never reads them (an agent's name/icon/colour/sort
 *  come from its `agents` definition via `agentId`). */
export type LegacySessionColumn =
  | `kind`
  | `dispatch`
  | `moveToDeviceId`
  | `handoffRestoredAt`
  | `caption`
  | `aside`
  | `actionId`
  | `actionName`
  | `actionInputs`
  | `spawnedBySessionId`
  | `name`
  | `icon`
  | `color`
  | `triggers`
  | `sortOrder`
/** A synced `sessions` row as the client reads it (no legacy columns). */
export type SessionRow = Omit<z.infer<typeof selectSyncedSessionSchema>, LegacySessionColumn>
/** A synced `session_issues` row; NEW-8: `assignedSessionId` is never read. */
export type SessionIssueRow = Omit<
  z.infer<typeof selectSyncedSessionIssueSchema>,
  `assignedSessionId`
>
/** A synced `messages` row. NEW-8: its legacy `sessionId` is never read on
 *  the client; the conversation decides (`keyMessageRows`). */
export type SyncedMessageRow = Omit<z.infer<typeof selectSyncedMessageSchema>, `sessionId`>
/** A message keyed to its conversation's session (`keyMessagesBySession`,
 *  fixture `message-conversation.json`): `sessionId` is the RESOLVED one. */
export type MessageRow = SyncedMessageRow & { sessionId: string }
export type PullRequestRow = z.infer<typeof selectPullRequestSchema>

const iso = (value: Date | string | null | undefined): string | null =>
  value == null ? null : value instanceof Date ? value.toISOString() : value

export function toTreeSession(row: SessionRow): TreeSession {
  return {
    id: row.id,
    teamId: row.teamId,
    parentId: row.parentId,
    rootId: row.rootId ?? row.id,
    agentId: row.agentId ?? null,
    appId: row.appId ?? null,
    userId: row.userId,
    deviceId: row.deviceId ?? null,
    status: row.status,
    busy: row.busy,
    background: row.background ?? false,
    needsInput: row.needsInput,
    blocked: row.blocked ?? null,
    shared: row.shared,
    title: row.title,
    guide: row.guide ?? null,
    startedAt: iso(row.startedAt),
    endedAt: iso(row.endedAt),
    createdAt: iso(row.createdAt) ?? new Date(0).toISOString(),
    updatedAt: iso(row.updatedAt) ?? new Date(0).toISOString(),
  }
}

export function toTreeSessionIssue(row: SessionIssueRow): TreeSessionIssue {
  return {
    sessionId: row.sessionId,
    issueId: row.issueId,
    sortOrder: row.sortOrder,
  }
}

export function toTreeIssue(row: { id: string; identifier: string; title: string }): TreeIssue {
  return { id: row.id, identifier: row.identifier, title: row.title }
}

export function toTreeMessage(row: MessageRow): TreeMessage {
  return {
    id: row.id,
    sessionId: row.sessionId,
    authorKind: row.authorKind,
    authorId: row.authorId,
    body: row.body,
    card: (row.card as Record<string, unknown> | null) ?? null,
    answeredAt: iso(row.answeredAt),
    consumedAt: iso(row.consumedAt),
    createdAt: iso(row.createdAt) ?? new Date(0).toISOString(),
    authorSessionId: row.authorSessionId ?? null,
  }
}

/** NEW-8: the synced rows keyed through `conversations` (conversation id →
 *  session id), oldest first; a dm/channel message or one whose
 *  conversation has not synced yet is held back. */
export function keyMessageRows(
  rows: readonly SyncedMessageRow[],
  conversations: ReadonlyMap<string, string>
): MessageRow[] {
  return keyMessagesBySession(rows, conversations).sort(
    (a, b) =>
      new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime() || (a.id < b.id ? -1 : 1)
  )
}

/** NEW-8: the conversation ids of `sessionIds` (each session's own), sorted:
 *  what a live query filters `messages.conversationId` on. */
export function sessionConversationIds(
  conversations: ReadonlyMap<string, string>,
  sessionIds: Iterable<string>
): string[] {
  const wanted = new Set(sessionIds)
  const ids: string[] = []
  for (const [conversationId, sessionId] of conversations) {
    if (wanted.has(sessionId)) ids.push(conversationId)
  }
  return ids.sort()
}

export function toTreePullRequest(row: PullRequestRow): TreePullRequest {
  return { id: row.id, number: row.number, state: row.state }
}
