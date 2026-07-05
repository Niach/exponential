import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { createCheckout } from "@creem_io/better-auth/server"
import { router, authedProcedure } from "@/lib/trpc"
import {
  countOwnedWorkspaces,
  getUserPlan,
  getWorkspacePlan,
  getWorkspaceUsage,
  FREE_OWNED_WORKSPACES_CAP,
  type PlanTier,
} from "@/lib/billing"
import { isCloudInstance } from "@/lib/bootstrap-cloud"
import { resolveWorkspaceAccess } from "@/lib/workspace-membership"

// The Creem product ids we allow a seat checkout to target. Gating here stops a
// caller from binding an arbitrary Creem product to a workspace they own.
function allowedProductIds(): Set<string> {
  return new Set(
    [
      process.env.CREEM_PRO_PRODUCT_ID,
      process.env.CREEM_BUSINESS_PRODUCT_ID,
      process.env.CREEM_BUSINESS_YEARLY_PRODUCT_ID,
    ].filter((id): id is string => Boolean(id))
  )
}

export const billingRouter = router({
  workspacePlan: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      if (!isCloudInstance()) {
        return {
          plan: `unlimited` as PlanTier,
          limits: {
            seats: Infinity,
            storageMb: Infinity,
            widgetConfigs: Infinity,
          },
          usage: { members: 0, storageMb: 0, widgetConfigs: 0 },
        }
      }

      // Only someone who can read the workspace may see its plan/usage.
      await resolveWorkspaceAccess(ctx.session.user.id, input.workspaceId)

      const [planData, usage] = await Promise.all([
        getWorkspacePlan(input.workspaceId),
        getWorkspaceUsage(input.workspaceId),
      ])

      return {
        ...planData,
        usage,
      }
    }),

  // Create a per-seat Creem checkout bound to a workspace. Only the workspace
  // owner may buy seats for it. We pass `units: seats` + metadata (workspaceId,
  // seats, referenceId) so the Creem plugin's webhook persistence binds the row
  // to the user (referenceId) while our onCheckoutCompleted/onGrantAccess hooks
  // bind it to the workspace + seat count (lib/billing/creem-binding.ts).
  createSeatCheckout: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        productId: z.string().min(1),
        seats: z.number().int().positive().max(1000),
        // Absolute URL Creem redirects to after payment. Defaults to the
        // billing settings page on this instance.
        successUrl: z.string().url().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      if (!isCloudInstance()) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `Billing is disabled on this instance`,
        })
      }
      const apiKey = process.env.CREEM_API_KEY
      if (!apiKey) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `Billing is not configured`,
        })
      }
      if (!allowedProductIds().has(input.productId)) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Unknown product`,
        })
      }

      // Only the workspace owner may purchase seats for it.
      await resolveWorkspaceAccess(
        ctx.session.user.id,
        input.workspaceId,
        `mutate_resources`,
        { roles: [`owner`] }
      )

      const successUrl =
        input.successUrl ??
        `${process.env.BETTER_AUTH_URL ?? ``}/settings/billing`

      const { url } = await createCheckout(
        {
          apiKey,
          testMode: apiKey.startsWith(`creem_test_`),
        },
        {
          productId: input.productId,
          units: input.seats,
          customer: { email: ctx.session.user.email ?? undefined },
          successUrl,
          // referenceId → the plugin's webhook persistence keys the row to this
          // user; workspaceId + seats → our binding hooks key it to the
          // workspace. Both survive Creem's metadata round-trip.
          metadata: {
            referenceId: ctx.session.user.id,
            workspaceId: input.workspaceId,
            seats: input.seats,
          },
        }
      )

      return { url }
    }),

  // User-scoped plan + owned-workspace usage, for pre-gating workspace
  // creation. `limit` is the invisible free-tier abuse cap (10 owned
  // workspaces); paid users are uncapped → null (Infinity→null convention).
  userPlan: authedProcedure.query(async ({ ctx }) => {
    const userId = ctx.session.user.id
    if (!isCloudInstance()) {
      return { plan: `unlimited` as PlanTier, ownedWorkspaces: 0, limit: null }
    }
    const [{ plan }, ownedWorkspaces] = await Promise.all([
      getUserPlan(userId),
      countOwnedWorkspaces(userId),
    ])
    return {
      plan,
      ownedWorkspaces,
      limit: plan === `free` ? FREE_OWNED_WORKSPACES_CAP : null,
    }
  }),
})
