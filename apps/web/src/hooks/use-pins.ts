import { useCallback, useMemo, useState } from "react"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { toast } from "sonner"
import type { PinKind } from "@exp/db-schema/domain"
import {
  actionCollection,
  codingSessionCollection,
  issueCollection,
  pinCollection,
} from "@/lib/collections"
import { sessionIdentity } from "@/lib/session-identity"
import { useTeamBoards } from "@/hooks/use-team-data"
import { trpc } from "@/lib/trpc-client"
import type { CodingSession, Issue, Pin, SyncedAction } from "@/db/schema"

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
    case `action`:
      return pin.actionId
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
// without a keyboard (Cmd+B opens the sidebar) — renders the same rows the
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
  | {
      pin: Pin
      kind: `session`
      session: CodingSession
      issue: Issue | null
      identifier: string | null
      title: string
    }
  | { pin: Pin; kind: `action`; action: SyncedAction; title: string }

export function usePinnedEntries(teamId: string | undefined): PinnedEntry[] {
  const pins = useTeamPins(teamId)
  const boards = useTeamBoards(teamId)
  const sessionIds = useMemo(
    () => pins.flatMap((pin) => (pin.sessionId ? [pin.sessionId] : [])),
    [pins]
  )
  const { data: sessions } = useLiveQuery(
    (query) =>
      sessionIds.length > 0
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) => inArray(s.id, sessionIds))
        : undefined,
    [sessionIds.join(`,`)]
  )
  // The pinned issues PLUS the pinned sessions' issues, so a session row can
  // name its identifier (the Sessions group's rule).
  const issueIds = useMemo(() => {
    const ids = new Set(pins.flatMap((pin) => (pin.issueId ? [pin.issueId] : [])))
    for (const session of (sessions ?? []) as CodingSession[]) {
      if (session.issueId) ids.add(session.issueId)
    }
    return [...ids].sort()
  }, [pins, sessions])
  const { data: issues } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => inArray(i.id, issueIds))
        : undefined,
    [issueIds.join(`,`)]
  )
  const { data: actions } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ a: actionCollection })
            .where(({ a }) => eq(a.teamId, teamId))
        : undefined,
    [teamId]
  )

  return useMemo(() => {
    const boardsById = new Map((boards ?? []).map((board) => [board.id, board]))
    const issuesById = new Map(
      ((issues ?? []) as Issue[]).map((issue) => [issue.id, issue])
    )
    const sessionsById = new Map(
      ((sessions ?? []) as CodingSession[]).map((session) => [
        session.id,
        session,
      ])
    )
    const actionsById = new Map(
      ((actions ?? []) as SyncedAction[]).map((action) => [action.id, action])
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
      if (pin.kind === `session` && pin.sessionId) {
        const session = sessionsById.get(pin.sessionId)
        if (!session) return []
        const issue = session.issueId
          ? (issuesById.get(session.issueId) ?? null)
          : null
        const identity = sessionIdentity({ session, issue: issue ?? undefined })
        return [
          {
            pin,
            kind: `session`,
            session,
            issue,
            identifier: identity.identifier,
            title: identity.identifier
              ? identity.subject
              : (session.actionName ??
                (session.issueId ? identity.subject : `Batch run`)),
          },
        ]
      }
      if (pin.kind === `action` && pin.actionId) {
        const action = actionsById.get(pin.actionId)
        if (!action) return []
        return [{ pin, kind: `action`, action, title: action.name }]
      }
      return []
    })
  }, [pins, boards, issues, sessions, actions])
}
