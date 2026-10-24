// FEED-87: an `issue_events` row as `exponential_issues_activity` returns it —
// per-field history a sync can replay (who changed what, when) instead of
// diffing the issue's single `updatedAt`. Pure, so the mapping is unit-tested
// without a database.
import type { IssueEvent } from "@/db/schema"

/** user = a person in an app; agent = an MCP call (a coding run when
 *  `sessionId` is set) or a team agent's session (#EXP-1383: `agent` names
 *  it); automation = the server (PR automation, webhooks). */
export type ActivityActorKind = `user` | `agent` | `automation`

export type ActivityRow = Pick<
  IssueEvent,
  | `id`
  | `type`
  | `payload`
  | `actorUserId`
  | `viaMcp`
  | `sessionId`
  | `createdAt`
> & {
  /** #EXP-1383: the acting team agent and its name (a deleted agent's row
   *  reads null). */
  actorAgentId?: string | null
  actorAgentName?: string | null
}

export interface ActivityWire {
  id: string
  type: string
  /** The field the event edits; null for `created`. Labels and relations
   *  are present/absent toggles, so `from`/`to` carry the id that left or
   *  arrived. */
  field: string | null
  from: unknown
  to: unknown
  actorId: string | null
  actorKind: ActivityActorKind
  /** #EXP-1383: the team agent that acted (absent = none). */
  agent?: { id: string; name: string | null }
  sessionId: string | null
  at: string
  /** The raw payload: status names, PR urls, relation details. */
  payload: Record<string, unknown> | null
}

type Payload = Record<string, unknown>

function fieldChange(
  type: string,
  payload: Payload
): { field: string | null; from: unknown; to: unknown } {
  switch (type) {
    case `status_changed`:
      // Status ROWS since EXP-314; pre-backfill rows only carry the enum.
      return {
        field: `status`,
        from: payload.fromStatusId ?? payload.from ?? null,
        to: payload.toStatusId ?? payload.to ?? null,
      }
    case `assignee_changed`:
      // #EXP-1383: an agent assignee reads `agent:<id>` (the picker's id).
      return {
        field: `assignee`,
        from: payload.from ?? (payload.fromAgentId ? `agent:${payload.fromAgentId}` : null),
        to: payload.to ?? (payload.toAgentId ? `agent:${payload.toAgentId}` : null),
      }
    case `priority_changed`:
    case `estimate_changed`:
    case `title_changed`:
      return {
        field: type.slice(0, -`_changed`.length),
        from: payload.from ?? null,
        to: payload.to ?? null,
      }
    case `label_added`:
      return { field: `label`, from: null, to: payload.labelId ?? null }
    case `label_removed`:
      return { field: `label`, from: payload.labelId ?? null, to: null }
    case `relation_added`:
      return { field: `relation`, from: null, to: payload.relatedIssueId ?? null }
    case `relation_removed`:
      return { field: `relation`, from: payload.relatedIssueId ?? null, to: null }
    case `board_moved`:
      return {
        field: `board`,
        from: payload.fromBoardId ?? null,
        to: payload.toBoardId ?? null,
      }
    case `pr_opened`:
      return { field: `pr`, from: null, to: payload.prUrl ?? null }
    case `pr_merged`:
      return { field: `pr`, from: payload.prUrl ?? null, to: `merged` }
    default:
      return { field: null, from: null, to: null }
  }
}

export function actorKindOf(row: ActivityRow): ActivityActorKind {
  const payload = (row.payload ?? {}) as Payload
  // pr-sync stamps `automation` on the status flip it makes for the team's
  // PR automation: the recorded actor opened/merged the PR, not the move.
  if (payload.automation === true) return `automation`
  if (row.actorAgentId) return `agent`
  if (row.actorUserId == null) return `automation`
  return row.viaMcp ? `agent` : `user`
}

export function activityWire(row: ActivityRow): ActivityWire {
  const payload = (row.payload ?? null) as Payload | null
  return {
    id: row.id,
    type: row.type,
    ...fieldChange(row.type, payload ?? {}),
    actorId: row.actorUserId,
    actorKind: actorKindOf(row),
    ...(row.actorAgentId
      ? { agent: { id: row.actorAgentId, name: row.actorAgentName ?? null } }
      : {}),
    sessionId: row.sessionId,
    at: new Date(row.createdAt).toISOString(),
    payload,
  }
}
