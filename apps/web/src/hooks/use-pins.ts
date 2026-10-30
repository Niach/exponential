import { useCallback, useMemo, useState } from "react"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { toast } from "@exp/ui"
import type { PinKind } from "@exp/db-schema/domain"
import { issueCollection, pinCollection } from "@/lib/collections"
import { useTeamBoards } from "@/hooks/use-team-data"
import { trpc } from "@/lib/trpc-client"
import type { Issue, Pin } from "@/db/schema"

export type { Pin }

// EXP-778: personal pins — the sidebar's "Pinned" group. The `pins` shape is
// per USER (static `user_id = me`), so the collection only ever holds the
// caller's rows; the hooks below narrow to the active team and let the
// sidebar resolve each target against the scoped collections (an issue on a
// trashed board, a session of a team the user left — no target, no row).

/** The caller's pins in a team, in display order (sort_order ascending). */
export function useTeamPins(teamId: string | undefined): Pin[] {
  const { data } = useLiveQuery(
    (query) =>
      teamId
        ? query.from({ p: pinCollection }).where(({ p }) => eq(p.teamId, teamId))
        : undefined,
    [teamId]
  )
  return useMemo(
    () => [...(data ?? [])].sort((a, b) => a.sortOrder - b.sortOrder),
    [data]
  )
}

function pinTargetId(pin: Pin): string | null {
  switch (pin.kind) {
    case `issue`:
      return pin.issueId
    case `session`:
      return pin.sessionId
    default:
      return null
  }
}

/** The caller's pin row for one target, if any. */
export function usePinFor(kind: PinKind, targetId: string | undefined) {
  const { data } = useLiveQuery(
    (query) =>
      targetId
        ? query.from({ p: pinCollection }).where(({ p }) => eq(p.kind, kind))
        : undefined,
    [kind, targetId]
  )
  return useMemo(
    () =>
      targetId
        ? (data ?? []).find((pin) => pinTargetId(pin) === targetId) ?? null
        : null,
    [data, targetId]
  )
}

/** Pin/unpin one target. `pinned` flips optimistically on click and settles
 *  on the synced row: the optimistic flag holds until the mutation's txId
 *  has landed in the pins collection (a failure resets it at once). */
export function usePinToggle(
  teamId: string | undefined,
  kind: PinKind,
  targetId: string | undefined
): { pinned: boolean; toggle: () => void; busy: boolean } {
  const pin = usePinFor(kind, targetId)
  const [optimistic, setOptimistic] = useState<boolean | null>(null)
  const [busy, setBusy] = useState(false)
  const synced = pin !== null
  const pinned = optimistic ?? synced

  const toggle = useCallback(() => {
    if (!teamId || !targetId || busy) return
    setBusy(true)
    setOptimistic(!synced)
    trpc.pins.toggle
      .mutate({ teamId, kind, targetId })
      // Clearing on the tRPC answer alone would let `pinned` flicker back to
      // the stale synced state until the Electric row arrives.
      .then(({ txId }) => pinCollection.utils.awaitTxId(txId))
      .then(() => setOptimistic(null))
      .catch(() => {
        setOptimistic(null)
        toast.error(`Could not update the pin`)
      })
      .finally(() => setBusy(false))
  }, [teamId, targetId, busy, synced, kind])

  return { pinned, toggle, busy }
}

// ── EXP-778 (EXP-818): the pinned rows, RESOLVED ─────────────────────────────
// A pin only renders when its target resolves, which every pin surface has to
// decide the same way. `usePinnedEntries` does the resolving once so a second
// surface — the phone's board-switcher sheet, which is the only Pinned there is
// on a phone — renders the same rows the
// sidebar group does.

export type PinnedEntry =
  | {
      pin: Pin
      kind: `issue`
      issue: Issue
      boardSlug: string
      identifier: string
      title: string
    }

export function usePinnedEntries(teamId: string | undefined): PinnedEntry[] {
  const pins = useTeamPins(teamId)
  const boards = useTeamBoards(teamId)
  // EXP-1293: a pinned SESSION resolves to nothing — the old coding-session
  // rows are gone, and a thread lives in the Threads list and Running.
  const issueIds = useMemo(
    () =>
      [...new Set(pins.flatMap((pin) => (pin.issueId ? [pin.issueId] : [])))].sort(),
    [pins]
  )
  const { data: issues } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => inArray(i.id, issueIds))
        : undefined,
    [issueIds.join(`,`)]
  )

  return useMemo(() => {
    const boardsById = new Map((boards ?? []).map((board) => [board.id, board]))
    const issuesById = new Map(
      ((issues ?? []) as Issue[]).map((issue) => [issue.id, issue])
    )
    return pins.flatMap((pin): PinnedEntry[] => {
      if (pin.kind === `issue` && pin.issueId) {
        const issue = issuesById.get(pin.issueId)
        const board = issue ? boardsById.get(issue.boardId) : undefined
        if (!issue || !board) return []
        return [
          {
            pin,
            kind: `issue`,
            issue,
            boardSlug: board.slug,
            identifier: issue.identifier,
            title: issue.title,
          },
        ]
      }
      return []
    })
  }, [pins, boards, issues])
}
