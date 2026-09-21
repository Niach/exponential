import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  actionCollection,
  attachmentCollection,
  automationCollection,
  boardCollection,
  codingSessionCollection,
  commentCollection,
  deviceCollection,
  labelCollection,
  notificationCollection,
  teamCollection,
  teamInviteCollection,
  userCollection,
  workflowCollection,
} from "@/lib/collections"
import { useIssueRefs, type ResolvedIssueRef } from "@/components/issue-ref-provider"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import type { StatusRowOption } from "@/lib/team-statuses"
import type { EntityRef } from "@/lib/mcp/preview"

// EXP-920: the synced row behind a tool-row chip, by kind — ONE live query
// per chip over the kind's own collection (skipped for the kinds that keep
// no row: a `list`, and the server-only `repository` and `thread`). An issue
// resolves through `IssueRefProvider` and a status through the team's rows,
// both already in context, so neither costs a query.
//
// `synced` is what the chip keys on: an unsynced row draws the same chip
// muted with no card (another team's board, a row deleted since the answer);
// the row-less kinds count as synced and draw a card from the ref itself.

/** The collection a kind's row lives in, and the column its ref id names
 *  (a device ref carries the machine's `device_id`, not the row id). Read
 *  per call, never at module load: the collections are a module a test may
 *  mock partially, and a table built at import would touch every one. */
function rowSource(
  kind: EntityRef[`kind`]
): { collection: typeof boardCollection; column: `id` | `deviceId` } | null {
  const of = (collection: unknown, column: `id` | `deviceId` = `id`) => ({
    collection: collection as typeof boardCollection,
    column,
  })
  switch (kind) {
    case `board`:
      return of(boardCollection)
    case `action`:
      return of(actionCollection)
    case `automation`:
      return of(automationCollection)
    case `comment`:
      return of(commentCollection)
    case `session`:
      return of(codingSessionCollection)
    case `label`:
      return of(labelCollection)
    case `workflow`:
      return of(workflowCollection)
    case `device`:
      return of(deviceCollection, `deviceId`)
    case `member`:
      return of(userCollection)
    case `team`:
      return of(teamCollection)
    case `invite`:
      return of(teamInviteCollection)
    case `notification`:
      return of(notificationCollection)
    case `attachment`:
      return of(attachmentCollection)
    default:
      return null
  }
}

export interface EntityRefRow<T = unknown> {
  /** The synced row (an issue: the `ResolvedIssueRef`; a status: the team's
   *  `StatusRowOption`), null when unknown here or when the kind keeps none. */
  row: T | null
  /** False = this client cannot show a card for the ref. */
  synced: boolean
}

export function useEntityRefRow(ref: EntityRef): EntityRefRow {
  const issueRefs = useIssueRefs()
  const statuses = useTeamStatusesContext()
  const source = rowSource(ref.kind)
  const id = ref.id
  const { data } = useLiveQuery(
    (query) =>
      // `undefined` (never `false`) is what skips a live query.
      source
        ? query
            .from({ rows: source.collection })
            .where(({ rows }) =>
              eq(
                (rows as unknown as Record<`id` | `deviceId`, string>)[source.column],
                id
              )
            )
        : undefined,
    [ref.kind, id]
  )

  return useMemo<EntityRefRow>(() => {
    switch (ref.kind) {
      case `issue`: {
        const row: ResolvedIssueRef | null =
          issueRefs?.resolveById(ref.id) ??
          (ref.identifier ? issueRefs?.resolve(ref.identifier) : null) ??
          issueRefs?.resolve(ref.id) ??
          null
        return { row, synced: row !== null }
      }
      case `status`: {
        const row: StatusRowOption | null = statuses.byId.get(ref.id) ?? null
        return { row, synced: row !== null }
      }
      case `list`:
      case `repository`:
      case `thread`:
        return { row: null, synced: true }
      default: {
        const row = (data as unknown[] | undefined)?.[0] ?? null
        return { row, synced: row !== null }
      }
    }
  }, [ref, issueRefs, statuses, data])
}
