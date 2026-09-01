import { useState } from "react"
import { toast } from "sonner"
import type { CodingSession } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

// EXP-688: killing a live run is offered from two places now — the mobile
// session view's "…" menu and the dock tab's X — so the confirmation dialog
// and the mutation live here instead of inside the session view. The caller
// renders `dialog` wherever it likes; the copy is identical either way.
//
// EXP-312: live implies ownership (the ticket mint refuses everyone else), so
// `canKill` is simply "the synced row is still going AND it is mine".

const LoadingIcon = conceptIcon(`ui-loading`)

export function useKillSession(
  session: Pick<CodingSession, `id` | `userId` | `status`>,
  currentUserId: string,
  /** The host machine's renamed label, named in the confirmation copy. */
  deviceLabel: string | null,
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
      // The synced row flips to ended — the dock keeps the panel mounted
      // until the user collapses it; the relay `bye` tears the socket down.
    } catch (error) {
      toast.error(`Couldn't kill the session`, {
        description: trpcErrorMessage(error, `The kill could not be delivered`),
      })
    } finally {
      setKilling(false)
    }
  }

  return {
    canKill,
    requestKill: () => setConfirmOpen(true),
    dialog: (
      <Dialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <DialogContent mobile="alert" className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Kill this coding session?</DialogTitle>
            <DialogDescription>
              This force-terminates the terminal
              {deviceLabel ? ` on ${deviceLabel}` : ``} and ends the session.
              Uncommitted work in the worktree is kept, but the agent stops
              immediately.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel
              onClick={() => setConfirmOpen(false)}
              disabled={killing}
            />
            <Button
              variant="destructive"
              onClick={() => void kill()}
              disabled={killing}
            >
              {killing && <LoadingIcon className="animate-spin" />}
              Kill session
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    ),
  }
}
