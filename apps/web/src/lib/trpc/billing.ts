import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import {
  countOwnedWorkspaces,
  getUserPlan,
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
            ownedWorkspaces: Infinity,
            push: true,
          },
          usage: { members: 0, projects: 0, storageMb: 0 },
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

  // User-scoped plan + owned-workspace usage, for pre-gating workspace creation.
  userPlan: authedProcedure.query(async ({ ctx }) => {
    const userId = ctx.session.user.id
    if (!isCloudInstance()) {
      return { plan: `unlimited` as PlanTier, ownedWorkspaces: 0, limit: Infinity }
    }
    const [{ plan, limits }, ownedWorkspaces] = await Promise.all([
      getUserPlan(userId),
      countOwnedWorkspaces(userId),
    ])
    return { plan, ownedWorkspaces, limit: limits.ownedWorkspaces }
  }),
})
