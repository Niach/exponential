import { TRPCError } from "@trpc/server"
import { authedProcedure, router } from "@/lib/trpc"

// SLOP-14 removes it.
export const workflowsRouter = router({
  create: authedProcedure.mutation(() => {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `Workflows were removed`,
    })
  }),
})
