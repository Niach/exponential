import type { ActionTrigger, ActionTriggerInput } from "@exp/db-schema/domain"
import { actionCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

/** A trigger as written: one the action already holds, or a new one the
 * server mints the id for. */
export type TriggerWrite = ActionTriggerInput | ActionTrigger

/**
 * Replaces an action's triggers (SLOP-2: `actions.update` takes the WHOLE
 * array, like inputs) and waits for the write to sync, so the row the caller
 * re-reads already carries it. `quiet` = the caller shows the error itself.
 */
export async function writeActionTriggers(
  actionId: string,
  triggers: TriggerWrite[],
  options: { quiet?: boolean } = {}
): Promise<void> {
  const result = await trpc.actions.update.mutate(
    { id: actionId, triggers },
    options.quiet ? { context: { skipErrorToast: true } } : undefined
  )
  if (`txId` in result && result.txId !== undefined) {
    await actionCollection.utils.awaitTxId(result.txId)
  }
}
