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
import { currentKeyHolder } from "@/lib/enet/current-key-holder"
import { openMessageWith, openTitleWith, resolveSealedMirrors } from "@/lib/enet/sealed-open"
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

/** A synced `sessions` row as the client reads it. */
export type SessionRow = z.infer<typeof selectSyncedSessionSchema>
/** A synced `session_issues` row. */
export type SessionIssueRow = z.infer<typeof selectSyncedSessionIssueSchema>
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
    startedByAgentId: row.startedByAgentId ?? null,
    appId: row.appId ?? null,
    userId: row.userId,
    deviceId: row.deviceId ?? null,
    status: row.status,
    busy: row.busy,
    background: row.background ?? false,
    needsInput: row.needsInput,
    blocked: row.blocked ?? null,
    shared: row.shared,
    // EXP-1459: a sealed title this install opens wins; else the neutral one.
    title: openTitleWith(row.sealedTitle, row.rootId ?? row.id, currentKeyHolder()) ?? row.title,
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
  // EXP-1459: sealed bodies and cards open here (or stay `locked`).
  return openMessageWith(plainTreeMessage(row), row.rootId, currentKeyHolder())
}

/** `toTreeMessage` over a list, with escalated origins this install cannot
 *  open stood in for by their mirror's sealed copy (`resolveSealedMirrors`). */
export function toTreeMessages(rows: readonly MessageRow[]): TreeMessage[] {
  const roots = new Map(rows.map((row) => [row.id, row.rootId]))
  return resolveSealedMirrors(rows.map(toTreeMessage), (message) => roots.get(message.id) ?? message.sessionId, currentKeyHolder())
}

function plainTreeMessage(row: MessageRow): TreeMessage {
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
