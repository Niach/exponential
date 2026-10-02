import { TRPCError } from "@trpc/server"
import { authedProcedure, router } from "@/lib/trpc"

// SLOP-3 compat stub: the "new workflow" button of iOS <= 0.14.50, Android <= 0.14.51 and
// desktop/CLI <= 0.14.59 calls it. SLOP-14 removes it once every floor passes those versions.
export const workflowsRouter = router({
  create: authedProcedure.mutation(() => {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `Workflows were removed`,
    })
  }),
})
