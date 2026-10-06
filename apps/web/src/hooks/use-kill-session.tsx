import { useState } from "react"
import { promptActions, stopRunPrompt } from "@/lib/prompts"
import type { CodingSession } from "@/db/schema"
import { Prompt, toast } from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"

// EXP-688: ending a live run is offered from two places now — the mobile
// session view's "…" menu and the dock tab's X — so the confirmation dialog
// and the mutation live here instead of inside the session view. The caller
// renders `dialog` wherever it likes; the copy is identical either way.
//
// EXP-849: the VERB is Stop, everywhere and on every client (the pill, this
// dialog, the natives' menus). "Kill" survives only where it names the wire
// (`steer.killSession`, `relayPostKill`) — a human never reads those.
//
// EXP-312: live implies ownership (the ticket mint refuses everyone else), so
// `canKill` is simply "the synced row is still going AND it is mine".

const STOP_RUN = stopRunPrompt()

export function useKillSession(
  session: Pick<CodingSession, `id` | `userId` | `status`>,
  currentUserId: string,
  /** EXP-550: the host stopped heartbeating — the run is parked, not live.
   * Killing it would end a run that resumes on its own when the machine
   * wakes, so a paused row is never killable from here. */
  paused: boolean
): { canKill: boolean; requestKill: () => void; dialog: React.ReactNode } {
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [killing, setKilling] = useState(false)

  const canKill =
    !paused &&
    session.userId === currentUserId &&
    (session.status === `running` || session.status === `in_review`)

  const kill = async () => {
    setKilling(true)
    try {
      await trpc.steer.killSession.mutate(
        { sessionId: session.id },
        { context: { skipErrorToast: true } }
      )
      setConfirmOpen(false)
      // The synced row flips to ended — the session page stays mounted and
      // read-only until the user leaves it; the relay `bye` tears the socket
      // down.
    } catch (error) {
      toast.error(`Couldn't stop the session`, {
        description: trpcErrorMessage(error, `The stop could not be delivered`),
      })
    } finally {
      setKilling(false)
    }
  }

  return {
    canKill,
    requestKill: () => setConfirmOpen(true),
    dialog: (
      <Prompt
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        busy={killing}
        title={STOP_RUN.title}
        actions={promptActions(STOP_RUN, {
          stop: { busy: killing, onSelect: kill },
        })}
      />
    ),
  }
}
