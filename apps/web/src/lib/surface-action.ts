import { EXPRESS_COPY, surfaceAction, type ActionEvent, type ActionOrigin } from "@exp/domain-contract/chat-express"
import { toastActionError } from "@exp/ui"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { sendSessionMessage } from "@/lib/enet/send-sealed"

/** #EXP-2067 (chat-express.json ACTIONS): a genUI surface's action event (an
 *  Express block's, an exponential_show card's) posts ONE person message
 *  into `sessionId` through the composer's send path: the event's text as
 *  the body, THE `action` card beside it (sealed in a keyed tree). Too much
 *  data = nothing posted, THE failed-action toast. */
export function postSurfaceAction(target: { teamId: string; sessionId: string }, event: ActionEvent, origin: ActionOrigin): void {
  const action = surfaceAction(event, origin)
  if (action === null) return
  if (action === `tooLarge`) {
    toastActionError(EXPRESS_COPY.actionTooLarge)
    return
  }
  void sendSessionMessage(
    { teamId: target.teamId, sessionId: target.sessionId, ...(action.body ? { text: action.body } : {}) },
    SKIP_ERROR_TOAST,
    { actionCard: action.card as unknown as Record<string, unknown> }
  ).catch((error) => failedAction(`send`, `the action`, error))
}
