// EXP-1293 (web): the session tree's view models are the
// contract's (`@exp/domain-contract/session-tree`, fixture-locked ×4); this
// re-exports them and maps the synced collection rows onto their inputs.

import { keyMessagesBySession } from "@exp/domain-contract/message-session"
import type { z } from "zod"
import type {
  selectPullRequestSchema,
  selectSyncedMessageSchema,
  selectSyncedSessionIssueSchema,
  selectSyncedSessionSchema,
} from "@exp/db-schema/schema"
import type { PrMergeState } from "@exp/domain-contract/merge-train"
import { currentKeySource } from "@/lib/enet/current-key-holder"
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
/** A synced `messages` row. #EXP-1730: its `sessionId` (trigger-kept from
 *  its conversation; null on a dm/channel row) is the session it lives in. */
export type SyncedMessageRow = z.infer<typeof selectSyncedMessageSchema>
/** A message of a session (`keyMessageRows`): `sessionId` is set. */
export type MessageRow = SyncedMessageRow & { sessionId: string }
export type PullRequestRow = z.infer<typeof selectPullRequestSchema>

/** #EXP-1710: a PR's merge-train state as the status pill, the `…` menu and
 *  Needs you read it (contract prMergeState; drizzle-zod infers the
 *  contract-array enum loosely): a merged or draft PR is that, whatever the
 *  train last wrote. */
export function prMergeState(pr: Pick<PullRequestRow, `state` | `mergeState`>): PrMergeState {
  if (pr.state === `merged`) return `merged`
  if (pr.state === `draft`) return `draft`
  return String(pr.mergeState) as PrMergeState
}

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
    title: openTitleWith(row.sealedTitle, row.rootId ?? row.id, currentKeySource()) ?? row.title,
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
  return openMessageWith(plainTreeMessage(row), row.rootId, currentKeySource())
}

/** `toTreeMessage` over a list, with escalated origins this install cannot
 *  open stood in for by their mirror's sealed copy (`resolveSealedMirrors`). */
export function toTreeMessages(rows: readonly MessageRow[]): TreeMessage[] {
  const roots = new Map(rows.map((row) => [row.id, row.rootId]))
  return resolveSealedMirrors(rows.map(toTreeMessage), (message) => roots.get(message.id) ?? message.sessionId, currentKeySource())
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
    // #EXP-1682: the clear files ride the row's bubble as embeds.
    attachmentIds: row.attachmentIds ?? [],
  }
}

/** #EXP-1730: the synced rows of a session (`messages.sessionId`; a
 *  dm/channel row carries none and is held back), oldest first. */
export function keyMessageRows(rows: readonly SyncedMessageRow[]): MessageRow[] {
  // fixtures/message-session.json
  return keyMessagesBySession(rows)
    .sort(
      (a, b) =>
        new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime() || (a.id < b.id ? -1 : 1)
    )
}

export function toTreePullRequest(row: PullRequestRow): TreePullRequest {
  return { id: row.id, number: row.number, state: row.state }
}
