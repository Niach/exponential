import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { issueDraftCollection } from "@/lib/collections"
import { useTeamBoards } from "@/hooks/use-team-data"
import { resolveDraftEntries, type DraftEntry } from "@/lib/issue-drafts"
import type { IssueDraft } from "@/db/schema"

// EXP-878: the React surface over the per-user `issue_drafts` shape. The
// collection only ever holds the caller's rows (static `user_id = me`), so
// these hooks only have to narrow to the active team and let the board
// resolve — the same "a row renders only when its target resolves" rule the
// pins surfaces follow.

/** The caller's drafts in a team, unresolved and unsorted. */
export function useTeamDrafts(teamId: string | undefined): IssueDraft[] {
  const { data } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ d: issueDraftCollection })
            .where(({ d }) => eq(d.teamId, teamId))
        : undefined,
    [teamId]
  )
  return useMemo(() => (data ?? []) as IssueDraft[], [data])
}

/** One draft by id — the create dialog's seed when it opens from a row. */
export function useIssueDraft(
  draftId: string | undefined
): IssueDraft | undefined {
  const { data } = useLiveQuery(
    (query) =>
      draftId
        ? query
            .from({ d: issueDraftCollection })
            .where(({ d }) => eq(d.id, draftId))
        : undefined,
    [draftId]
  )
  return useMemo(() => ((data ?? [])[0] as IssueDraft | undefined), [data])
}

/**
 * The RESOLVED rows every drafts surface renders — the list, the sidebar
 * entry's visibility and its count all read this one answer, so a draft on a
 * board the viewer can no longer see is invisible everywhere at once.
 */
export function useDraftEntries(teamId: string | undefined): DraftEntry[] {
  const drafts = useTeamDrafts(teamId)
  const boards = useTeamBoards(teamId)
  return useMemo(
    () => resolveDraftEntries(drafts, boards ?? [], teamId),
    [drafts, boards, teamId]
  )
}
