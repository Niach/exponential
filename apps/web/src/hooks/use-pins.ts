import { useCallback, useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { toast } from "sonner"
import type { PinKind } from "@exp/db-schema/domain"
import { pinCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import type { Pin } from "@/db/schema"

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
 *  on the synced row (the server answer resets it on failure). */
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
      .then(() => setOptimistic(null))
      .catch(() => {
        setOptimistic(null)
        toast.error(`Could not update the pin`)
      })
      .finally(() => setBusy(false))
  }, [teamId, targetId, busy, synced, kind])

  return { pinned, toggle, busy }
}
