import { router } from "@/lib/trpc"
import { workflowCreateProcedures } from "./create"
import { workflowNodeProcedures } from "./nodes"
import { workflowReviewProcedures } from "./review"
import { workflowFinalPrProcedures } from "./final-pr"
import { workflowEventProcedures } from "./events"

// EXP-981 workflows router, split by concern (EXP-1082): `shared.ts` holds
// the module-level helpers, each file one procedure group. Procedure names
// are the wire contract and stay exactly as they were.
export const workflowsRouter = router({
  ...workflowCreateProcedures,
  ...workflowNodeProcedures,
  ...workflowReviewProcedures,
  ...workflowFinalPrProcedures,
  ...workflowEventProcedures,
})

export {
  WORKFLOW_DEVICE_CAP,
  appendDecisionLine,
  launchFromDeviceDefaults,
  mergeBelongsToAttempt,
  mergedNodeOutcome,
  normalizeLaunchLenient,
  reviewOutcome,
  storedLaunchFor,
} from "./shared"
