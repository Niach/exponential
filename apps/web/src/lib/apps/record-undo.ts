// VAPP-215 (R4, app-records.json `history`): the Undo toast a UI records
// write raises (the Data tab's add / save / delete, a package action's
// batch): the existing success toast + its action for `undoSeconds` s.
// Undo = `apps.records.undo` (each event's `before` as a new write).

import { APP_HISTORY_COPY, APP_RECORD_HISTORY, undoTitle } from "@exp/domain-contract/app-records"
import { toast } from "@exp/ui"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { appRecordCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

export type UndoableWrite = `create` | `update` | `delete` | `batch`

/** Toast `write` with Undo; no events (a batch that changed nothing) = no toast. */
export function toastRecordUndo(appId: string, write: UndoableWrite, eventIds: readonly string[], via: `editor` | `app`, title = undoTitle(write)): void {
  if (eventIds.length === 0) return
  toast.success(title, {
    duration: APP_RECORD_HISTORY.undoSeconds * 1000,
    action: {
      label: APP_HISTORY_COPY.undo,
      onClick: () => void undoRecordWrite(appId, eventIds, via),
    },
  })
}

/** Run the undo; a refusal is the failed-action toast. */
export async function undoRecordWrite(appId: string, eventIds: readonly string[], via: `editor` | `app`): Promise<void> {
  try {
    const result = await trpc.apps.records.undo.mutate({ id: appId, eventIds: [...eventIds], via }, SKIP_ERROR_TOAST)
    await appRecordCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
  } catch (error) {
    failedAction(APP_HISTORY_COPY.undoVerb, APP_HISTORY_COPY.undoThing, error)
  }
}
