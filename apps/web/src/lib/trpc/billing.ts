import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import {
  getWorkspacePlan,
  getWorkspaceUsage,
  type PlanTier,
} from "@/lib/billing"
import { isCloudInstance } from "@/lib/bootstrap-cloud"

export const billingRouter = router({
  workspacePlan: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ input }) => {
      if (!isCloudInstance()) {
        return {
          plan: `unlimited` as PlanTier,
          limits: {
            members: Infinity,
            projects: Infinity,
            storageMb: Infinity,
            push: true,
          },
          usage: { members: 0, projects: 0 },
        }
      }

      const [planData, usage] = await Promise.all([
        getWorkspacePlan(input.workspaceId),
        getWorkspaceUsage(input.workspaceId),
      ])

      return {
        ...planData,
        usage,
      }
    }),
})
