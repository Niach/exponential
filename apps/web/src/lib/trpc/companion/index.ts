import { router } from "@/lib/trpc"
import { setupProcedures } from "./setup"
import { hubProcedures } from "./hub"
import { identityProcedures } from "./identity"

export const companionRouter = router({
  ...setupProcedures,
  ...hubProcedures,
  ...identityProcedures,
})
