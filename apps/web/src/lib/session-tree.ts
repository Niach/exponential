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
import type {
  TreeIssue,
  TreeMessage,
  TreePullRequest,
  TreeSession,
  TreeSessionIssue,
} from "@exp/domain-contract/session-tree"

export * from "@exp/domain-contract/session-tree"
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

export type SessionRow = z.infer<typeof selectSyncedSessionSchema>
export type SessionIssueRow = z.infer<typeof selectSyncedSessionIssueSchema>
export type MessageRow = z.infer<typeof selectSyncedMessageSchema>
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

export function toTreePullRequest(row: PullRequestRow): TreePullRequest {
  return { id: row.id, number: row.number, state: row.state }
}
