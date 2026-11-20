import { useMemo } from "react"
import { useParams } from "@tanstack/react-router"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { boardCollection, issueCollection, issueDraftCollection } from "@/lib/collections"
import { useTeamBoardsWithReady } from "@/hooks/use-team-data"
import { resolveDraftEntries, type DraftEntry } from "@/lib/issue-drafts"
import type { Board, Issue, IssueDraft } from "@/db/schema"

// EXP-878: the React surface over the per-user `issue_drafts` shape. The
// collection only ever holds the caller's rows (static `user_id = me`), so
// these hooks only have to narrow to the active team and let the board
// resolve — the same "a row renders only when its target resolves" rule the
// pins surfaces follow.

/** The caller's drafts in a team, unresolved and unsorted, plus whether the
 *  shape has delivered its first snapshot (a DISABLED query reports ready,
 *  so `teamId` gates it too). */
export function useTeamDraftsWithReady(teamId: string | undefined): {
  drafts: IssueDraft[]
  isReady: boolean
} {
  const { data, isReady } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ d: issueDraftCollection })
            .where(({ d }) => eq(d.teamId, teamId))
        : undefined,
    [teamId]
  )
  const drafts = useMemo(() => (data ?? []) as IssueDraft[], [data])
  return { drafts, isReady: Boolean(teamId) && isReady }
}

/** The caller's drafts in a team, unresolved and unsorted. */
export function useTeamDrafts(teamId: string | undefined): IssueDraft[] {
  return useTeamDraftsWithReady(teamId).drafts
}

/** One draft by id — the New issue page's seed when it reopens a row. */
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
 * EXP-1231: the issue CREATED FROM a draft, if any — `issues.draft_id` is
 * stamped by `issues.create({ draftId })` in the transaction that deletes
 * the row. The New issue page watches it: an issue here means the draft was
 * filed on another client, and the page lands on it.
 */
export function useIssueFromDraft(draftId: string | undefined): Issue | undefined {
  const { data } = useLiveQuery(
    (query) =>
      draftId
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => eq(i.draftId, draftId))
        : undefined,
    [draftId]
  )
  return useMemo(() => ((data ?? [])[0] as Issue | undefined), [data])
}

/**
 * The RESOLVED rows every drafts surface renders — the list, the sidebar
 * entry's visibility and its count all read this one answer, so a draft on a
 * board the viewer can no longer see is invisible everywhere at once.
 */
export function useDraftEntries(teamId: string | undefined): DraftEntry[] {
  return useDraftEntriesWithReady(teamId).entries
}

/**
 * EXP-1170: how many drafts the sidebar's Drafts entry counts — every
 * resolved draft EXCEPT the one open on the New issue page right now (it is
 * the page, not something parked). Zero hides the entry.
 */
export function useSidebarDraftCount(teamId: string | undefined): number {
  const { draftId } = useParams({ strict: false }) as { draftId?: string }
  const entries = useDraftEntries(teamId)
  return draftId
    ? entries.filter((entry) => entry.draft.id !== draftId).length
    : entries.length
}

/**
 * `useDraftEntries` plus readiness: both the drafts and the boards shape have
 * delivered their first snapshot. A surface that REDIRECTS away when no draft
 * exists (the phone inbox's `?tab=drafts`) must wait for this, or a cold deep
 * link bounces to the inbox before the rows have even arrived.
 */
export function useDraftEntriesWithReady(teamId: string | undefined): {
  entries: DraftEntry[]
  isReady: boolean
} {
  const { drafts, isReady: draftsReady } = useTeamDraftsWithReady(teamId)
  const { boards, boardsReady } = useTeamBoardsWithReady(teamId)
  const entries = useMemo(
    () => resolveDraftEntries(drafts, boards ?? [], teamId),
    [drafts, boards, teamId]
  )
  return { entries, isReady: draftsReady && boardsReady }
}

/**
 * #EXP-2027 (sidebar-tree.json ROOT `drafts`): the caller's drafts in EVERY
 * listed team (the team-less Drafts page of web md+), each resolved like
 * `useDraftEntriesWithReady` against its own team's boards, newest first.
 */
export function useCrossTeamDraftEntriesWithReady(teamIds: readonly string[]): {
  entries: DraftEntry[]
  isReady: boolean
} {
  const key = teamIds.join(`,`)
  const { data: draftRows, isReady: draftsReady } = useLiveQuery(
    (query) =>
      teamIds.length > 0
        ? query.from({ d: issueDraftCollection }).where(({ d }) => inArray(d.teamId, [...teamIds]))
        : undefined,
    [key]
  )
  const { data: boardRows, isReady: boardsReady } = useLiveQuery(
    (query) =>
      teamIds.length > 0
        ? query.from({ boards: boardCollection }).where(({ boards }) => inArray(boards.teamId, [...teamIds]))
        : undefined,
    [key]
  )
  const entries = useMemo(() => {
    const drafts = (draftRows ?? []) as IssueDraft[]
    const boards = (boardRows ?? []) as Board[]
    return teamIds
      .flatMap((teamId) => resolveDraftEntries(drafts, boards, teamId))
      .sort((left, right) => new Date(right.draft.updatedAt).getTime() - new Date(left.draft.updatedAt).getTime())
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draftRows, boardRows, key])
  return { entries, isReady: teamIds.length > 0 && draftsReady && boardsReady }
}

/** #EXP-2027: the root Drafts row's count over every listed team (the
 *  draft open on the New issue page right now excluded, EXP-1170). */
export function useCrossTeamDraftCount(teamIds: readonly string[]): number {
  const { draftId } = useParams({ strict: false }) as { draftId?: string }
  const { entries } = useCrossTeamDraftEntriesWithReady(teamIds)
  return draftId ? entries.filter((entry) => entry.draft.id !== draftId).length : entries.length
}
